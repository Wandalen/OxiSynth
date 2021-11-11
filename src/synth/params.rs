use crate::Synth;

use crate::oxi::{InterpolationMethod, Settings};

/**
Synthesis parameters
 */
impl Synth
{
  /**
  Get a reference to the settings of the synthesizer.
   */
  pub fn settings(&self) -> &Settings { &self.handle.settings() }

  /**
  Set the master gain
   */
  pub fn set_gain(&mut self, gain : f32) { self.handle.set_gain(gain) }

  /**
  Get the master gain
   */
  pub fn gain(&self) -> f32 { self.handle.gain() }

  /**
  Set the polyphony limit
   */
  pub fn set_polyphony(&mut self, polyphony : u16) -> Result<(), ()> { self.handle.set_polyphony(polyphony) }

  /**
  Get the polyphony limit (FluidSynth >= 1.0.6)
   */
  pub fn polyphony(&self) -> u32 { self.handle.polyphony() }

  /**
  Get the internal buffer size. The internal buffer size if not the
  same thing as the buffer size specified in the
  settings. Internally, the synth *always* uses a specific buffer
  size independent of the buffer size used by the audio driver. The
  internal buffer size is normally 64 samples. The reason why it
  uses an internal buffer size is to allow audio drivers to call the
  synthesizer with a variable buffer length. The internal buffer
  size is useful for client who want to optimize their buffer sizes.
   */
  pub fn internal_buffer_size(&self) -> usize { self.handle.internal_bufsize() }

  /** Set the interpolation method for one channel (`Some(chan)`) or all channels (`None`) */
  pub fn set_interp_method(&mut self, chan : Option<usize>, interp_method : InterpolationMethod)
  {
    self.handle.set_interp_method(chan, interp_method)
  }
}
