//! CPAL-based audio output
//!
//! Cross-platform audio output using the `cpal` crate.
//! Works on macOS, Windows, Linux, iOS, Android.

#[cfg(feature = "audio-cpal")]
mod implementation {
    use super::super::output::{
        AudioCallback, AudioDevice, AudioOutput, AudioOutputError, OutputState,
    };
    use crate::audio::format::{AudioFormat, SampleFormat, SampleRate};
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use std::sync::{Arc, Mutex, mpsc};
    use std::thread;
    use std::time::Duration;

    enum StreamCommand {
        Pause,
        Resume,
        Stop,
    }

    /// CPAL-based audio output implementation
    pub struct CpalOutput {
        host: cpal::Host,
        device: Option<cpal::Device>,
        command_tx: Option<mpsc::Sender<StreamCommand>>,
        state: OutputState,
        volume: Arc<Mutex<f32>>,
        format: Option<AudioFormat>,
        callback: Arc<Mutex<Option<AudioCallback>>>,
    }

    impl CpalOutput {
        /// Create a new CPAL output
        ///
        /// # Errors
        ///
        /// Returns `AudioOutputError` if the audio host cannot be initialized.
        pub fn new() -> Result<Self, AudioOutputError> {
            let host = cpal::default_host();

            Ok(Self {
                host,
                device: None,
                command_tx: None,
                state: OutputState::Stopped,
                volume: Arc::new(Mutex::new(1.0)),
                format: None,
                callback: Arc::new(Mutex::new(None)),
            })
        }

        #[allow(clippy::too_many_arguments)]
        fn spawn_stream_thread(
            device: cpal::Device,
            config: cpal::StreamConfig,
            sample_format: SampleFormat,
            callback_ref: Arc<Mutex<Option<AudioCallback>>>,
            volume_ref: Arc<Mutex<f32>>,
            rx: mpsc::Receiver<StreamCommand>,
            status_tx: mpsc::Sender<Result<(), AudioOutputError>>,
        ) {
            thread::spawn(move || {
                let err_fn = |err| tracing::error!("CPAL stream error: {}", err);

                let stream_result = Self::build_stream(
                    &device,
                    &config,
                    sample_format,
                    callback_ref,
                    volume_ref,
                    err_fn,
                );

                match stream_result {
                    Ok(stream) => {
                        if let Err(e) = stream.play() {
                            let _ =
                                status_tx.send(Err(AudioOutputError::StreamError(e.to_string())));
                            return;
                        }

                        // Notify success
                        if status_tx.send(Ok(())).is_err() {
                            return; // Caller dropped receiver
                        }

                        Self::run_command_loop(stream.as_ref(), &rx);
                    }
                    Err(e) => {
                        let _ = status_tx.send(Err(e));
                    }
                }
            });
        }

        fn build_stream<E>(
            device: &cpal::Device,
            config: &cpal::StreamConfig,
            sample_format: SampleFormat,
            callback_ref: Arc<Mutex<Option<AudioCallback>>>,
            volume_ref: Arc<Mutex<f32>>,
            err_fn: E,
        ) -> Result<Box<dyn StreamTrait>, AudioOutputError>
        where
            E: Fn(cpal::StreamError) + Send + 'static + Copy,
        {
            match sample_format {
                SampleFormat::I16 => {
                    let stream = device
                        .build_output_stream(
                            config,
                            move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                                let bytes: &mut [u8] = unsafe {
                                    std::slice::from_raw_parts_mut(
                                        data.as_mut_ptr().cast::<u8>(),
                                        data.len() * 2,
                                    )
                                };

                                let mut cb = callback_ref.lock().unwrap();
                                if let Some(ref mut callback) = *cb {
                                    callback(bytes);
                                }

                                // Apply volume
                                let vol = *volume_ref.lock().unwrap();
                                for sample in data.iter_mut() {
                                    #[allow(clippy::cast_possible_truncation)]
                                    {
                                        *sample = (f32::from(*sample) * vol) as i16;
                                    }
                                }
                            },
                            err_fn,
                            None,
                        )
                        .map_err(|e| AudioOutputError::DeviceError(e.to_string()))?;
                    Ok(Box::new(stream))
                }
                SampleFormat::F32 => {
                    let stream = device
                        .build_output_stream(
                            config,
                            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                                let bytes: &mut [u8] = unsafe {
                                    std::slice::from_raw_parts_mut(
                                        data.as_mut_ptr().cast::<u8>(),
                                        data.len() * 4,
                                    )
                                };

                                let mut cb = callback_ref.lock().unwrap();
                                if let Some(ref mut callback) = *cb {
                                    callback(bytes);
                                }

                                // Apply volume
                                let vol = *volume_ref.lock().unwrap();
                                for sample in data.iter_mut() {
                                    *sample *= vol;
                                }
                            },
                            err_fn,
                            None,
                        )
                        .map_err(|e| AudioOutputError::DeviceError(e.to_string()))?;
                    Ok(Box::new(stream))
                }
                _ => Err(AudioOutputError::FormatNotSupported(AudioFormat {
                    sample_format,
                    sample_rate: SampleRate::Hz44100, // Dummy
                    channels: crate::audio::format::ChannelConfig::Stereo, // Dummy
                })),
            }
        }

        fn run_command_loop(stream: &dyn StreamTrait, rx: &mpsc::Receiver<StreamCommand>) {
            loop {
                match rx.recv() {
                    Ok(StreamCommand::Stop) | Err(_) => break, // Channel closed
                    Ok(StreamCommand::Pause) => {
                        let _ = stream.pause();
                    }
                    Ok(StreamCommand::Resume) => {
                        let _ = stream.play();
                    }
                }
            }
        }
    }

    impl AudioOutput for CpalOutput {
        fn enumerate_devices(&self) -> Result<Vec<AudioDevice>, AudioOutputError> {
            let default_name = self
                .host
                .default_output_device()
                .map(|d| d.name().unwrap_or_default());

            let devices = self
                .host
                .output_devices()
                .map_err(|e| AudioOutputError::DeviceError(e.to_string()))?;

            let mut result = Vec::new();
            for device in devices {
                let name = device.name().unwrap_or_else(|_| "Unknown".to_string());
                let is_default = default_name.as_ref() == Some(&name);

                // Get supported configs
                let mut supported_rates = Vec::new();
                let mut supported_channels = Vec::new();

                if let Ok(configs) = device.supported_output_configs() {
                    for config in configs {
                        // Add min and max sample rates
                        if let Some(rate) = SampleRate::from_hz(config.min_sample_rate().0) {
                            supported_rates.push(rate);
                        }
                        if let Some(rate) = SampleRate::from_hz(config.max_sample_rate().0) {
                            supported_rates.push(rate);
                        }
                        if let Ok(channels) = u8::try_from(config.channels()) {
                            supported_channels.push(channels);
                        }
                    }
                }

                supported_rates.sort_by_key(|r| r.as_u32());
                supported_rates.dedup();
                supported_channels.sort_unstable();
                supported_channels.dedup();

                result.push(AudioDevice {
                    id: name.clone(),
                    name,
                    is_default,
                    supported_rates,
                    supported_channels,
                });
            }

            Ok(result)
        }

        fn default_device(&self) -> Result<AudioDevice, AudioOutputError> {
            let device = self
                .host
                .default_output_device()
                .ok_or_else(|| AudioOutputError::DeviceNotFound("No default device".into()))?;

            let name = device.name().unwrap_or_else(|_| "Default".to_string());

            Ok(AudioDevice {
                id: name.clone(),
                name,
                is_default: true,
                supported_rates: vec![SampleRate::Hz44100, SampleRate::Hz48000],
                supported_channels: vec![2],
            })
        }

        fn open(
            &mut self,
            device_id: Option<&str>,
            format: AudioFormat,
        ) -> Result<(), AudioOutputError> {
            let device = if let Some(id) = device_id {
                self.host
                    .output_devices()
                    .map_err(|e| AudioOutputError::DeviceError(e.to_string()))?
                    .find(|d| d.name().ok().as_deref() == Some(id))
                    .ok_or_else(|| AudioOutputError::DeviceNotFound(id.to_string()))?
            } else {
                self.host
                    .default_output_device()
                    .ok_or_else(|| AudioOutputError::DeviceNotFound("No default".into()))?
            };

            self.device = Some(device);
            self.format = Some(format);

            Ok(())
        }

        fn start(&mut self, callback: AudioCallback) -> Result<(), AudioOutputError> {
            if self.command_tx.is_some() {
                let _ = self.stop();
            }

            let device = self
                .device
                .as_ref()
                .ok_or_else(|| AudioOutputError::DeviceError("No device opened".into()))?
                .clone();

            let format = self
                .format
                .ok_or_else(|| AudioOutputError::DeviceError("No format set".into()))?;

            // Store callback
            *self.callback.lock().unwrap() = Some(callback);
            let callback_ref = self.callback.clone();
            let volume_ref = self.volume.clone();

            // Configure stream
            let config = cpal::StreamConfig {
                channels: u16::from(format.channels.channels()),
                sample_rate: cpal::SampleRate(format.sample_rate.as_u32()),
                buffer_size: cpal::BufferSize::Default,
            };

            let sample_format = format.sample_format;

            let (tx, rx) = mpsc::channel();
            self.command_tx = Some(tx);

            let (status_tx, status_rx) = mpsc::channel();

            // Spawn thread
            Self::spawn_stream_thread(
                device,
                config,
                sample_format,
                callback_ref,
                volume_ref,
                rx,
                status_tx,
            );

            // Wait for initialization
            status_rx
                .recv()
                .map_err(|_| AudioOutputError::DeviceError("Audio thread panicked".into()))??;

            self.state = OutputState::Playing;

            Ok(())
        }

        fn stop(&mut self) -> Result<(), AudioOutputError> {
            if let Some(tx) = self.command_tx.take() {
                let _ = tx.send(StreamCommand::Stop);
            }
            self.state = OutputState::Stopped;
            Ok(())
        }

        fn pause(&mut self) -> Result<(), AudioOutputError> {
            if let Some(ref tx) = self.command_tx {
                let _ = tx.send(StreamCommand::Pause);
            }
            self.state = OutputState::Paused;
            Ok(())
        }

        fn resume(&mut self) -> Result<(), AudioOutputError> {
            if let Some(ref tx) = self.command_tx {
                let _ = tx.send(StreamCommand::Resume);
            }
            self.state = OutputState::Playing;
            Ok(())
        }

        fn state(&self) -> OutputState {
            self.state
        }

        fn set_volume(&mut self, volume: f32) -> Result<(), AudioOutputError> {
            *self.volume.lock().unwrap() = volume.clamp(0.0, 1.0);
            Ok(())
        }

        fn volume(&self) -> f32 {
            *self.volume.lock().unwrap()
        }

        fn latency(&self) -> Duration {
            // CPAL doesn't expose latency directly; estimate
            Duration::from_millis(50)
        }

        fn format(&self) -> Option<AudioFormat> {
            self.format
        }

        fn close(&mut self) -> Result<(), AudioOutputError> {
            self.stop()
        }
    }
}

#[cfg(feature = "audio-cpal")]
pub use implementation::CpalOutput;
