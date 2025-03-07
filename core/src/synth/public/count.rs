use crate::synth::Synth;

impl Synth
{
  /**
  Returns the number of MIDI channels that the synthesizer uses internally
   */
  pub fn count_midi_channels(&self) -> usize { self.channels.len() }

  /**
  Returns the number of audio channels that the synthesizer uses internally
   */
  pub fn count_audio_channels(&self) -> u8 { self.settings.audio_channels }

  /**
  Returns the number of audio groups that the synthesizer uses internally.
  This is usually identical to audio_channels.
   */
  pub fn count_audio_groups(&self) -> u8 { self.settings.audio_groups }

  /**
  Returns the number of effects channels that the synthesizer uses internally
   */
  pub fn count_effects_channels(&self) -> u8 { 2 }
}
