use std::convert::TryInto;

use crate::chorus::Chorus;
use crate::error::OxiError;
use crate::reverb::Reverb;
use crate::soundfont::modulator::Mod;
use crate::soundfont::{
  generator::{gen_scale_nrpn, GeneratorType},
  InstrumentZone, PresetZone, SoundFont,
};
use crate::synth::channel_pool::Channel;
use crate::synth::font_bank::FontBank;
use crate::synth::voice_pool::{Voice, VoiceAddMode, VoiceDescriptor, VoicePool};
use crate::utils::TypedIndex;

type GenType = u32;
const GEN_LAST : GenType = 60;

type MidiControlChange = u32;
const RPN_MSB : MidiControlChange = 101;
const RPN_LSB : MidiControlChange = 100;
const NRPN_MSB : MidiControlChange = 99;
const NRPN_LSB : MidiControlChange = 98;
const DATA_ENTRY_LSB : MidiControlChange = 38;

/**
Send a noteon message.
 */
pub fn noteon(
  channel : &Channel,
  voices : &mut VoicePool,
  start_time : usize,
  min_note_length_ticks : usize,
  gain : f32,
  key : u8,
  vel : u8,
) -> Result<(), OxiError>
{
  if vel == 0
  {
    noteoff(channel, voices, min_note_length_ticks, key);
    Ok(())
  }
  else if channel.preset().is_none()
  {
    Err(OxiError::ChannelHasNoPreset)
  }
  else
  {
    voices.release_voice_on_same_note(channel, key, min_note_length_ticks);
    voices.noteid_add();

    inner_noteon(channel, voices, start_time, gain, key, vel);
    Ok(())
  }
}

fn inner_noteon(channel : &Channel, voices : &mut VoicePool, start_time : usize, gain : f32, key : u8, vel : u8)
{
  fn preset_zone_inside_range(zone : &PresetZone, key : u8, vel : u8) -> bool
  {
    zone.key_low <= key && zone.key_high >= key && zone.vel_low <= vel && zone.vel_high >= vel
  }

  fn inst_zone_inside_range(zone : &InstrumentZone, key : u8, vel : u8) -> bool
  {
    zone.key_low <= key && zone.key_high >= key && zone.vel_low <= vel && zone.vel_high >= vel
  }

  let preset = &channel.preset().unwrap();

  // list for 'sorting' preset modulators
  let mod_list_new : Vec<Option<&Mod>> = (0 .. 64).into_iter().map(|_| None).collect();
  let mut mod_list : [Option<&Mod>; 64] = mod_list_new.try_into().unwrap();

  let mut global_preset_zone = preset.global_zone();

  // run thru all the zones of this preset
  for preset_zone in preset.zones().iter()
  {
    // check if the note falls into the key and velocity range of this preset
    if preset_zone_inside_range(preset_zone, key, vel)
    {
      let inst = preset_zone.inst.as_ref().unwrap();

      let mut global_inst_zone = &inst.global_zone();

      // run thru all the zones of this instrument
      for inst_zone in inst.zones().iter()
      {
        // make sure this instrument zone has a valid sample
        let sample = &inst_zone.sample;
        if !(sample.is_none() || sample.as_ref().unwrap().sample_type.is_rom())
        {
          // check if the note falls into the key and velocity range of this instrument
          if inst_zone_inside_range(inst_zone, key, vel) && !sample.is_none()
          {
            // this is a good zone. allocate a new synthesis process and initialize it

            // Initialize Voice
            let init = |voice : &mut Voice| {
              voice.add_default_mods();

              // Instrument level, generators
              for i in 0 .. GEN_LAST
              {
                use num_traits::FromPrimitive;
                /* SF 2.01 section 9.4 'bullet' 4:
                 *
                 * A generator in a local instrument zone supersedes a
                 * global instrument zone generator.  Both cases supersede
                 * the default generator -> voice_gen_set */
                if inst_zone.gen[i as usize].flags != 0
                {
                  voice.gen_set(FromPrimitive::from_u8(i as u8).unwrap(), inst_zone.gen[i as usize].val);
                }
                else if let Some(global_inst_zone) = &global_inst_zone
                {
                  if global_inst_zone.gen[i as usize].flags as i32 != 0
                  {
                    voice.gen_set(FromPrimitive::from_u8(i as u8).unwrap(), global_inst_zone.gen[i as usize].val);
                  }
                }
                else
                {
                  /* The generator has not been defined in this instrument.
                   * Do nothing, leave it at the default.
                   */
                }
              }

              /* global instrument zone, modulators: Put them all into a
               * list. */
              let mut mod_list_count = 0;
              if let Some(global_inst_zone) = &mut global_inst_zone
              {
                for m in global_inst_zone.mods.iter()
                {
                  mod_list[mod_list_count] = Some(m);
                  mod_list_count += 1;
                }
              }

              /* local instrument zone, modulators.
               * Replace modulators with the same definition in the list:
               * SF 2.01 page 69, 'bullet' 8
               */
              for m in inst_zone.mods.iter()
              {
                /* 'Identical' modulators will be deleted by setting their
                 *  list entry to NULL.  The list length is known, NULL
                 *  entries will be ignored later.  SF2.01 section 9.5.1
                 *  page 69, 'bullet' 3 defines 'identical'.  */
                for i in 0 .. mod_list_count
                {
                  if mod_list[i].is_some() && m.test_identity(mod_list[i as usize].as_ref().unwrap())
                  {
                    mod_list[i] = None;
                  }
                }

                /* Finally add the new modulator to to the list. */
                mod_list[mod_list_count] = Some(m);

                mod_list_count += 1;
              }

              // Add instrument modulators (global / local) to the voice.
              for i in 0 .. mod_list_count
              {
                let mod_0 = mod_list[i as usize];
                if mod_0.is_some()
                {
                  // disabled modulators CANNOT be skipped.

                  /* Instrument modulators -supersede- existing (default)
                   * modulators.  SF 2.01 page 69, 'bullet' 6 */
                  voice.add_mod(mod_0.as_ref().unwrap(), VoiceAddMode::Overwrite);
                }
              }

              const GEN_STARTADDROFS : u32 = 0;
              const GEN_ENDADDROFS : u32 = 1;
              const GEN_STARTLOOPADDROFS : u32 = 2;
              const GEN_ENDLOOPADDROFS : u32 = 3;
              const GEN_STARTADDRCOARSEOFS : u32 = 4;

              const GEN_ENDADDRCOARSEOFS : u32 = 12;

              const GEN_STARTLOOPADDRCOARSEOFS : u32 = 45;
              const GEN_KEYNUM : u32 = 46;
              const GEN_VELOCITY : u32 = 47;

              const GEN_ENDLOOPADDRCOARSEOFS : u32 = 50;
              const GEN_SAMPLEMODE : u32 = 54;
              const GEN_EXCLUSIVECLASS : u32 = 57;
              const GEN_OVERRIDEROOTKEY : u32 = 58;
              const GEN_LAST : u32 = 60;

              /* Preset level, generators */
              for i in 0 .. GEN_LAST
              {
                /* SF 2.01 section 8.5 page 58: If some generators are
                 * encountered at preset level, they should be ignored */
                if i != GEN_STARTADDROFS
                  && i != GEN_ENDADDROFS
                  && i != GEN_STARTLOOPADDROFS
                  && i != GEN_ENDLOOPADDROFS
                  && i != GEN_STARTADDRCOARSEOFS
                  && i != GEN_ENDADDRCOARSEOFS
                  && i != GEN_STARTLOOPADDRCOARSEOFS
                  && i != GEN_KEYNUM
                  && i != GEN_VELOCITY
                  && i != GEN_ENDLOOPADDRCOARSEOFS
                  && i != GEN_SAMPLEMODE
                  && i != GEN_EXCLUSIVECLASS
                  && i != GEN_OVERRIDEROOTKEY
                {
                  /* SF 2.01 section 9.4 'bullet' 9: A generator in a
                   * local preset zone supersedes a global preset zone
                   * generator.  The effect is -added- to the destination
                   * summing node -> voice_gen_incr */
                  if preset_zone.gen[i as usize].flags != 0
                  {
                    voice.gen_incr(i, preset_zone.gen[i as usize].val);
                  }
                  else if let Some(global_preset_zone) = &global_preset_zone
                  {
                    if global_preset_zone.gen[i as usize].flags != 0
                    {
                      voice.gen_incr(i, global_preset_zone.gen[i as usize].val);
                    }
                  }
                  else
                  {
                    /* The generator has not been defined in this preset
                     * Do nothing, leave it unchanged.
                     */
                  }
                } /* if available at preset level */
              } /* for all generators */

              /* Global preset zone, modulators: put them all into a
               * list. */
              let mut mod_list_count = 0;
              if let Some(global_preset_zone) = &mut global_preset_zone
              {
                for m in global_preset_zone.mods.iter()
                {
                  mod_list[mod_list_count] = Some(m);
                  mod_list_count += 1;
                }
              }

              /* Process the modulators of the local preset zone.  Kick
               * out all identical modulators from the global preset zone
               * (SF 2.01 page 69, second-last bullet) */
              for m in preset_zone.mods.iter()
              {
                for i in 0 .. mod_list_count
                {
                  if mod_list[i].is_some() && m.test_identity(mod_list[i as usize].as_ref().unwrap())
                  {
                    mod_list[i] = None;
                  }
                }

                /* Finally add the new modulator to the list. */
                mod_list[mod_list_count] = Some(m);

                mod_list_count += 1;
              }

              // Add preset modulators (global / local) to the voice.
              for i in 0 .. mod_list_count
              {
                if let Some(m) = mod_list[i]
                {
                  if m.amount != 0.0
                  {
                    // disabled modulators can be skipped.

                    /* Preset modulators -add- to existing instrument /
                     * default modulators.  SF2.01 page 70 first bullet on
                     * page */
                    voice.add_mod(m, VoiceAddMode::Add);
                  }
                }
              }

              /* Store the ID of the first voice that was created by this noteon event.
               * Exclusive class may only terminate older voices.
               * That avoids killing voices, which have just been created.
               * (a noteon event can create several voice processes with the same exclusive
               * class - for example when using stereo samples)
               */
            };

            let desc = VoiceDescriptor {
              sample : sample.as_ref().unwrap().clone(),
              channel,
              key,
              vel,
              start_time,
              gain,
            };

            let voice_id = voices.request_new_voice(desc, init);

            if let Ok(_) = voice_id
            {
              log::trace!(
                "noteon\t{}\t{}\t{}\t\t{}",
                channel.id(),
                key,
                vel,
                start_time as f32 / 44100.0,
              );
            }
            else
            {
              log::warn!("Failed to allocate a synthesis process. (chan={},key={})", channel.id(), key);
            }
          }
        }
      }
    }
  }
}

/**
Send a noteoff message.
 */
pub fn noteoff(channel : &Channel, voices : &mut VoicePool, min_note_length_ticks : usize, key : u8)
{
  voices.noteoff(channel, min_note_length_ticks, key)
}

/**
Send a control change message.
 */
pub fn cc(
  channel : &mut Channel,
  voices : &mut VoicePool,
  min_note_length_ticks : usize,
  drums_channel_active : bool,
  num : u8,
  value : u8,
)
{
  *channel.cc_mut(num as usize) = value;

  match num
  {
    // SUSTAIN_SWITCH
    64 =>
    {
      if value < 64
      {
        // sustain off
        voices.damp_voices(channel, min_note_length_ticks)
      }
      else
      {
        // sustain on
      }
    }

    // BANK_SELECT_MSB
    0 =>
    {
      if channel.id() == 9 && drums_channel_active
      {
        // ignored
        return;
      }

      channel.set_bank_msb((value & 0x7f) as u8);

      /* I fixed the handling of a MIDI bank select controller 0,
      e.g., bank select MSB (or "coarse" bank select according to
      my spec).  Prior to this fix a channel's bank number was only
      changed upon reception of MIDI bank select controller 32,
      e.g, bank select LSB (or "fine" bank-select according to my
      spec). [KLE]
      FIXME: is this correct? [PH] */
      channel.set_banknum((value & 0x7f) as u32);
    }

    // BANK_SELECT_LSB
    32 =>
    {
      if channel.id() == 9 && drums_channel_active
      {
        // ignored
        return;
      }

      /* FIXME: according to the Downloadable Sounds II specification,
      bit 31 should be set when we receive the message on channel
      10 (drum channel) */
      channel.set_banknum((value as u32 & 0x7f).wrapping_add((channel.bank_msb() as u32) << 7 as i32));
    }

    // ALL_NOTES_OFF
    123 =>
    {
      all_notes_off(channel, voices, min_note_length_ticks);
    }

    // ALL_SOUND_OFF
    120 =>
    {
      all_notes_off(channel, voices, min_note_length_ticks);
    }

    // ALL_CTRL_OFF
    121 =>
    {
      channel.init_ctrl(1);
      voices.modulate_voices_all(channel);
    }

    // DATA_ENTRY_MSB
    6 =>
    {
      let data : i32 = ((value as i32) << 7 as i32) + channel.cc(DATA_ENTRY_LSB as usize) as i32;

      if channel.nrpn_active() != 0
      {
        let (nrpn_select, nrpn_msb, nrpn_lsb) = (
          channel.nrpn_select(),
          channel.cc(NRPN_MSB as usize),
          channel.cc(NRPN_LSB as usize),
        );

        // SontFont 2.01 NRPN Message (Sect. 9.6, p. 74)
        if nrpn_msb == 120 && nrpn_lsb < 100
        {
          if (nrpn_select as i32) < GEN_LAST as i32
          {
            use num_traits::FromPrimitive;

            let scale_nrpn : f32 = gen_scale_nrpn(nrpn_select, data);

            let param = FromPrimitive::from_u8(nrpn_select as u8).unwrap();
            super::gen::set_gen(channel, voices, param, scale_nrpn)
          }

          channel.set_nrpn_select(0); // Reset to 0
        }
      }
      /* RPN is active: MSB = 0? */
      else if channel.cc(RPN_MSB as usize) == 0
      {
        match channel.cc(RPN_LSB as usize)
        {
          // RPN_PITCH_BEND_RANGE
          0 => pitch_wheel_sens(channel, voices, value),
          // RPN_CHANNEL_FINE_TUNE
          1 =>
          {
            super::gen::set_gen(
              channel,
              voices,
              GeneratorType::FineTune,
              ((data - 8192) as f64 / 8192.0f64 * 100.0f64) as f32,
            );
          }
          // RPN_CHANNEL_COARSE_TUNE
          2 =>
          {
            super::gen::set_gen(channel, voices, GeneratorType::CoarseTune, (value - 64) as f32);
          }
          // RPN_TUNING_PROGRAM_CHANGE | RPN_TUNING_BANK_SELECT | RPN_MODULATION_DEPTH_RANGE
          3 | 4 | 5 | _ =>
          {}
        }
      }
    }

    // NRPN_MSB
    99 =>
    {
      *channel.cc_mut(NRPN_LSB as usize) = 0;
      channel.set_nrpn_select(0);
      channel.set_nrpn_active(1);
    }

    // NRPN_LSB
    98 =>
    {
      // SontFont 2.01 NRPN Message (Sect. 9.6, p. 74)
      if channel.cc(NRPN_MSB as usize) == 120
      {
        if value == 100
        {
          channel.set_nrpn_select(channel.nrpn_select() + 100);
        }
        else if value == 101
        {
          channel.set_nrpn_select(channel.nrpn_select() + 1000);
        }
        else if value == 102
        {
          channel.set_nrpn_select(channel.nrpn_select() + 10000);
        }
        else if value < 100
        {
          channel.set_nrpn_select(channel.nrpn_select() + value as i16);
        }
      }
      channel.set_nrpn_active(1);
    }

    // RPN_MSB | RPN_LSB
    101 | 100 => channel.set_nrpn_active(0),
    _ => voices.modulate_voices(channel, true, num),
  }
}

/**
Get a control value.
 */
// pub fn get_cc(channel: &Channel, num: u16) -> u8 {
//     channel.cc(num as usize)
// }

pub fn all_notes_off(channel : &Channel, voices : &mut VoicePool, min_note_length_ticks : usize)
{
  voices.all_notes_off(channel, min_note_length_ticks)
}

pub fn all_sounds_off(channel_id : usize, voices : &mut VoicePool) { voices.all_sounds_off(channel_id) }

/**
Send a pitch bend message.
 */
pub fn pitch_bend(channel : &mut Channel, voices : &mut VoicePool, val : u16)
{
  const MOD_PITCHWHEEL : u8 = 14;

  channel.set_pitch_bend(val as i16);
  voices.modulate_voices(&channel, false, MOD_PITCHWHEEL);
}

// /**
// Get the pitch bend value.
//  */
// pub fn get_pitch_bend(channel: &Channel) -> i16 {
//     channel.pitch_bend()
// }

/**
Set the pitch wheel sensitivity.
 */
pub fn pitch_wheel_sens(channel : &mut Channel, voices : &mut VoicePool, val : u8)
{
  const MOD_PITCHWHEELSENS : u8 = 16;

  channel.set_pitch_wheel_sensitivity(val);
  voices.modulate_voices(&channel, false, MOD_PITCHWHEELSENS);
}

// /**
// Get the pitch wheel sensitivity.
//  */
// pub fn get_pitch_wheel_sens(channel: &Channel) -> u8 {
//     channel.pitch_wheel_sensitivity()
// }

/**
Send a program change message.
 */
pub fn program_change(channel : &mut Channel, font_bank : &FontBank, program_id : u8, drums_channel_active : bool)
{
  let banknum = channel.banknum();
  channel.set_prognum(program_id);

  let mut preset = if channel.id() == 9 && drums_channel_active
  {
    font_bank.find_preset(128, program_id)
  }
  else
  {
    font_bank.find_preset(banknum, program_id)
  };

  if preset.is_none()
  {
    let mut subst_bank = banknum as i32;
    let mut subst_prog = program_id;
    if banknum != 128
    {
      subst_bank = 0;
      preset = font_bank.find_preset(0, program_id);
      if preset.is_none() && program_id != 0
      {
        preset = font_bank.find_preset(0, 0);
        subst_prog = 0;
      }
    }
    else
    {
      preset = font_bank.find_preset(128, 0);
      subst_prog = 0;
    }
    if preset.is_none()
    {
      log::warn!(
        "Instrument not found on channel {} [bank={} prog={}], substituted [bank={} prog={}]",
        channel.id(),
        banknum,
        program_id,
        subst_bank,
        subst_prog
      );
    }
  }

  channel.set_sfontnum(preset.as_ref().map(|p| p.0));
  channel.set_preset(preset.map(|p| p.1.clone()));
}

/**
Set channel pressure
 */
pub fn channel_pressure(channel : &mut Channel, voices : &mut VoicePool, val : u8)
{
  const MOD_CHANNELPRESSURE : u8 = 13;

  channel.set_channel_pressure(val);
  voices.modulate_voices(&channel, false, MOD_CHANNELPRESSURE);
}

/**
Set key pressure (aftertouch)
 */
pub fn key_pressure(channel : &mut Channel, voices : &mut VoicePool, key : u8, val : u8)
{
  channel.set_key_pressure(key as usize, val as i8);
  voices.key_pressure(&channel, key);
}

/**
Select a bank.
 */
pub fn bank_select(channel : &mut Channel, bank : u32) { channel.set_banknum(bank); }

/**
Select a sfont.
 */
pub fn sfont_select(channel : &mut Channel, sfont_id : TypedIndex<SoundFont>) { channel.set_sfontnum(Some(sfont_id)); }

/**
Select a preset for a channel. The preset is specified by the
SoundFont ID, the bank number, and the preset number. This
allows any preset to be selected and circumvents preset masking
due to previously loaded SoundFonts on the SoundFont stack.
 */
pub fn program_select(
  channel : &mut Channel,
  font_bank : &FontBank,
  sfont_id : TypedIndex<SoundFont>,
  bank_id : u32,
  preset_id : u8,
) -> Result<(), OxiError>
{
  let preset = font_bank.preset(sfont_id, bank_id, preset_id);

  if preset.is_none()
  {
    log::error!(
      "There is no preset with bank number {} and preset number {} in SoundFont {:?}",
      bank_id,
      preset_id,
      sfont_id
    );
    Err(OxiError::PresetNotFound {
      bank_id,
      preset_id,
      sfont_id,
    })
  }
  else
  {
    channel.set_sfontnum(Some(sfont_id));
    channel.set_banknum(bank_id);
    channel.set_prognum(preset_id);
    channel.set_preset(preset);
    Ok(())
  }
}

/**
Returns the program, bank, and SoundFont number of the preset on a given channel.
 */
pub fn get_program(channel : &Channel) -> (Option<TypedIndex<SoundFont>>, u32, u32)
{
  (channel.sfontnum(), channel.banknum(), channel.prognum() as u32)
}

/**
Send a bank select and a program change to every channel to reinitialize the preset of the channel.

This function is useful mainly after a SoundFont has been loaded, unloaded or reloaded.
 */
pub fn program_reset(channels : &mut [Channel], font_bank : &FontBank, drums_channel_active : bool)
{
  for channel in channels.iter_mut()
  {
    program_change(channel, font_bank, channel.prognum(), drums_channel_active);
  }
}

/**
Send a reset.

A reset turns all the notes off and resets the controller values.

Purpose:
Respond to the MIDI command 'system reset' (0xFF, big red 'panic' button)
 */
pub fn system_reset(
  voices : &mut VoicePool,
  channels : &mut [Channel],
  font_bank : &FontBank,
  chorus : &mut Chorus,
  reverb : &mut Reverb,
)
{
  voices.system_reset();

  let preset = font_bank.find_preset(0, 0).map(|p| p.1);
  for channel in channels.iter_mut()
  {
    channel.init(preset.clone());
    channel.init_ctrl(0);
  }

  chorus.reset();
  reverb.reset();
}
