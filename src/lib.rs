//! # Rsynth
//! A crate for developing audio plugins and applications in Rust, with a focus on software synthesis.
//! Rsynth is well suited as a bootstrap for common audio plugin generators.
//! It handles voices, voice-stealing, polyphony, etc. so the programmer's main focus can be DSP.
//!
//! ## Back-ends
//! `rsynth` currently supports the following back-ends:
//!
//! * [`combined`] combine different back-ends for audio input, audio output, midi input and
//!     midi output, mostly for offline rendering and testing (behind various features)
//! * [`jack`] (behind the `backend-jack` feature)
//! * [`vst`] (behind the backend-vst)
//!
//! See the documentation of each back-end for more information.
//!
//! ## Rendering audio
//! Audio can be rendered by using a number of traits:
//!
//! * the [`AudioRenderer`] trait
//! * the [`ContextualAudioRenderer`] trait
//!
//! These traits are very similar, the [`ContextualAudioRenderer`] trait adds one extra parameter
//! that defines a "context" that can be passed to the implementor of the trait, so that the
//! implementor of the trait does not need to own all data that is needed for rendering the audio;
//! it can also borrow some data with additional the `context` parameter.
//!
//! Both traits are generic over the data type that represents the sample.
//! For which precise data-type an application or plugin needs to implement the trait, depends on
//! the back-end. Because the trait is generic, the application or plugin can have a generic implementation
//! as well that can be used by different back-ends.
//!
//! ## Meta-data
//! There are a number of traits that an application or plugin needs to implement in order to define meta-data.
//! Every plugin should implement these, but it can be tedious, so you can implement these
//! traits in a more straightforward way by implementing the [`Meta`] trait.
//! However, you can also implement these trait "by hand":
//!
//! * [`CommonPluginMeta`]
//!     * Name of the plugin etc
//! * [`AudioHandlerMeta`]
//!     * Number of audio ports
//! * [`MidiHandlerMeta`]
//!     * Number of midi ports
//! * [`CommonAudioPortMeta`]
//!     * Names of the audio in and out ports
//! * [`CommonPluginMeta`]
//!     * Name of the plugin or application
//!
//! Additionally, back-ends can require extra trait bounds related to meta-data.
//!
//! ## Handling events
//! Plugins or application can handle events by implementing a number of traits:
//!
//! * [`EventHandler`]
//! * [`ContextualEventHandler`]
//!
//! Both traits are generic over the event type.
//! These traits are very similar, the [`ContextualEventHandler`] trait adds one extra parameter
//! that defines a "context" that can be passed to the implementor of the trait, so that the
//! implementor of the trait does not need to own all data that is needed for handling the event;
//! it can also borrow some data with additional the `context` parameter.
//!
//! ## Events
//! `rsynth` defines a number of event types:
//!
//! * [`RawMidiEvent`]: a raw MIDI event
//! * [`SysExEvent`]: a system exclusive event
//! * [`Timed<T>`]: a timed event
//! * [`Indexed<T>`]:
//!
//! ## Utilities
//! Utilities are are types that you can include to perform several common tasks for the
//! plugin or application:
//!
//! * polyphony: managing of different voices
//!
//! [`Plugin`]: ./trait.Plugin.html
//! [`jack`]: ./backend/jack_backend/index.html
//! [`vst`]: ./backend/vst_backend/index.html
//! [`combined`]: ./backend/combined/index.html
//! [`EventHandler`]: ./event/trait.EventHandler.html
//! [`RawMidiEvent`]: ./event/struct.RawMidiEvent.html
//! [`SysExEvent`]: ./event/struct.SysExEvent.html
//! [`Timed<T>`]: ./event/struct.Timed.html
//! [`Indexed<T>`]: ./event/struct.Indexed.html
//! [`CommonPluginMeta`]: ./trait.CommonPluginMeta.html
//! [`AudioHandlerMeta`]: ./trait.AudioHandlerMeta.html
//! [`MidiHandlerMeta`]: ./trait.MidiHandlerMeta.html
//! [`CommonAudioPortMeta`]: ./trait.CommonAudioPortMeta.html
//! [`Meta`]: ./meta/trait.Meta.html
//! [`AudioRenderer`]: ./trait.AudioRenderer.html
//! [`ContextualEventHandler`]: ./event/trait.ContextualEventHandler.html
//! [`EventHandler`]: ./event/trait.EventHandler.html

#[macro_use]
extern crate log;
extern crate asprim;
extern crate num_traits;
extern crate vecstorage;

#[cfg(feature = "backend-file-hound")]
extern crate hound;
#[cfg(feature = "backend-jack")]
extern crate jack;
#[cfg(feature = "backend-file-hound")]
extern crate sample;
#[cfg(feature = "backend-vst")]
extern crate vst;

#[macro_use]
extern crate doc_comment;

use crate::meta::{AudioPort, General, Meta, MidiPort, Name, Port};

#[macro_use]
pub mod buffer;
pub mod backend;
pub mod envelope;
pub mod event;
pub mod meta;
pub mod test_utilities;
pub mod utilities;

doctest!("../README.md");

/// Define the maximum number of audio inputs and the maximum number of audio outputs.
///
/// Backends that require the plugin to implement this trait ensure that when calling the
/// [`render_buffer`] method of the [`AudioRenderer`] trait
/// *  the number of inputs (`inputs.len()`) is smaller than or equal to
///    `Self::max_number_of_audio_inputs()` and
/// * the number of outputs (`outputs.len()`) is smaller than or equal to
///    `Self::max_number_of_audio_outputs()`.
///
/// # Remark
/// This trait can be more conveniently implemented by implementing the [`Meta`] trait.
///
/// [`Meta`]: ./meta/trait.Meta.html
/// [`render_buffer`]: ./trait.AudioHandlerMeta.html#tymethod.render_buffer
/// [`AudioRenderer`]: ./trait.AudioHandlerMeta.html
pub trait AudioHandlerMeta {
    /// The maximum number of audio inputs supported.
    /// This method should return the same value every time it is called.
    fn max_number_of_audio_inputs(&self) -> usize;

    /// The maximum number of audio outputs supported.
    /// This method should return the same value every time it is called.
    fn max_number_of_audio_outputs(&self) -> usize;
}

/// Define how sample-rate changes are handled.
pub trait AudioHandler {
    /// Called when the sample-rate changes.
    /// The backend should ensure that this function is called before
    /// any other method.
    ///
    /// # Parameters
    /// `sample_rate`: The new sample rate in frames per second (Hz).
    /// Common sample rates are 44100 Hz (CD quality) and 48000 Hz.
    // TODO: Looking at the WikiPedia list https://en.wikipedia.org/wiki/Sample_rate, it seems that
    // TODO: there are no fractional sample rates. Maybe change the data type into u32?
    fn set_sample_rate(&mut self, sample_rate: f64);
}

/// Define the maximum number of midi inputs and the maximum number of midi outputs.
/// This trait can be more conveniently implemented by implementing the [`Meta`] trait.
///
/// [`Meta`]: ./meta/trait.Meta.html
pub trait MidiHandlerMeta {
    /// The maximum number of midi inputs supported.
    /// This method should return the same value for subsequent calls.
    fn max_number_of_midi_inputs(&self) -> usize;

    /// The maximum number of midi outputs supported.
    /// This method should return the same value for subsequent calls.
    fn max_number_of_midi_outputs(&self) -> usize;
}

/// Defines how audio is rendered.
///
/// The type parameter `S` refers to the data type of a sample.
/// It is typically `f32` or `f64`.
pub trait AudioRenderer<S> {
    /// This method is called repeatedly for subsequent audio buffers.
    ///
    /// The lengths of all elements of `inputs` and the lengths of all elements of `outputs`
    /// are all guaranteed to equal to each other.
    /// This shared length can however be different for subsequent calls to `render_buffer`.
    fn render_buffer(&mut self, inputs: &[&[S]], outputs: &mut [&mut [S]]);
}

/// Defines how audio is rendered, similar to the [`AudioRenderer`] trait.
/// The extra parameter `context` can be used by the backend to provide extra information.
///
/// See the documentation of [`AudioRenderer`] for more information.
///
/// [`AudioRenderer`]: ./trait.AudioHandlerMeta.html
pub trait ContextualAudioRenderer<S, Context> {
    /// This method called repeatedly for subsequent buffers.
    ///
    /// It is similar to the [`render_buffer`] from the [`AudioRenderer`] trait,
    /// see its documentation for more information.
    ///
    /// [`AudioRenderer`]: ./trait.AudioHandlerMeta.html
    /// [`render_buffer`]: ./trait.AudioHandlerMeta.html#tymethod.render_buffer
    fn render_buffer(&mut self, inputs: &[&[S]], outputs: &mut [&mut [S]], context: &mut Context);
}

/// Provides common meta-data of the plugin or application to the host.
/// This trait is common for all backends that need this info.
/// This trait can be more conveniently implemented by implementing the [`Meta`] trait.
///
/// [`Meta`]: ./meta/trait.Meta.html
pub trait CommonPluginMeta {
    /// The name of the plugin or application.
    fn name(&self) -> &str;
}

/// Provides some meta-data of the audio-ports used by the plugin or application to the host.
/// This trait can be more conveniently implemented by implementing the [`Meta`] trait.
///
/// [`Meta`]: ./meta/trait.Meta.html
pub trait CommonAudioPortMeta: AudioHandlerMeta {
    /// The name of the audio input with the given index.
    /// You can assume that `index` is strictly smaller than [`Self::max_number_of_audio_inputs()`].
    ///
    /// # Note
    /// When using the Jack backend, this function should not return an empty string.
    ///
    /// [`Self::max_number_of_audio_inputs()`]: trait.AudioHandlerMeta.html#tymethod.max_number_of_audio_inputs
    fn audio_input_name(&self, index: usize) -> String {
        format!("audio in {}", index)
    }

    /// The name of the audio output with the given index.
    /// You can assume that `index` is strictly smaller than [`Self::max_number_of_audio_outputs()`].
    ///
    /// # Note
    /// When using the Jack backend, this function should not return an empty string.
    ///
    /// [`Self::max_number_of_audio_outputs()`]: ./trait.AudioHandlerMeta.html#tymethod.max_number_of_audio_outputs
    fn audio_output_name(&self, index: usize) -> String {
        format!("audio out {}", index)
    }
}

/// Provides some meta-data of the midi-ports used by the plugin or application to the host.
/// This trait can be more conveniently implemented by implementing the [`Meta`] trait.
///
/// [`Meta`]: ./meta/trait.Meta.html
pub trait CommonMidiPortMeta: MidiHandlerMeta {
    /// The name of the midi input with the given index.
    /// You can assume that `index` is strictly smaller than [`Self::max_number_of_midi_inputs()`].
    ///
    /// # Note
    /// When using the Jack backend, this function should not return an empty string.
    ///
    /// [`Self::max_number_of_midi_inputs()`]: trait.MidiHandlerMeta.html#tymethod.max_number_of_midi_inputs
    fn midi_input_name(&self, index: usize) -> String {
        format!("midi in {}", index)
    }

    /// The name of the midi output with the given index.
    /// You can assume that `index` is strictly smaller than [`Self::max_number_of_midi_outputs()`]
    ///
    /// # Note
    /// When using the Jack backend, this function should not return an empty string.
    ///
    /// [`Self::max_number_of_midi_outputs()`]: ./trait.MidiHandlerMeta.html#tymethod.max_number_of_midi_outputs
    fn midi_output_name(&self, index: usize) -> String {
        format!("midi out {}", index)
    }
}

impl<T> CommonPluginMeta for T
where
    T: Meta,
    T::MetaData: General,
    <<T as Meta>::MetaData as General>::GeneralData: Name,
{
    fn name(&self) -> &str {
        self.meta().general().name()
    }
}

impl<T> AudioHandlerMeta for T
where
    T: Meta,
    T::MetaData: Port<AudioPort>,
{
    fn max_number_of_audio_inputs(&self) -> usize {
        self.meta().in_ports().len()
    }

    fn max_number_of_audio_outputs(&self) -> usize {
        self.meta().out_ports().len()
    }
}

impl<T> CommonAudioPortMeta for T
where
    T: Meta,
    T::MetaData: Port<AudioPort>,
    <<T as Meta>::MetaData as Port<AudioPort>>::PortData: Name,
{
    fn audio_input_name(&self, index: usize) -> String {
        self.meta().in_ports()[index].name().to_string()
    }

    fn audio_output_name(&self, index: usize) -> String {
        self.meta().out_ports()[index].name().to_string()
    }
}

impl<T> MidiHandlerMeta for T
where
    T: Meta,
    T::MetaData: Port<MidiPort>,
{
    fn max_number_of_midi_inputs(&self) -> usize {
        self.meta().in_ports().len()
    }

    fn max_number_of_midi_outputs(&self) -> usize {
        self.meta().out_ports().len()
    }
}

impl<T> CommonMidiPortMeta for T
where
    T: Meta,
    T::MetaData: Port<MidiPort>,
    <<T as Meta>::MetaData as Port<MidiPort>>::PortData: Name,
{
    fn midi_input_name(&self, index: usize) -> String {
        // TODO: It doesn't feel right that we have to do a `to_string` here.
        self.meta().in_ports()[index].name().to_string()
    }

    fn midi_output_name(&self, index: usize) -> String {
        // TODO: It doesn't feel right that we have to do a `to_string` here.
        self.meta().out_ports()[index].name().to_string()
    }
}
