use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::time::Duration;

use airplay2::types::AirPlayDevice;
use airplay2::types::DeviceCapabilities;
use airplay2::types::raop::{RaopCapabilities, RaopCodec, RaopEncryption};
use tokio::fs;
use tokio::task::JoinHandle;

use crate::common::ports::reserve_ports;
use crate::common::subprocess::{
    ReadyStrategy, SubprocessConfig, SubprocessError, SubprocessHandle, TimestampedLogLine,
};

#[derive(Debug)]
#[allow(dead_code)]
pub enum AudioError {
    InvalidData(String),
}

#[allow(dead_code)]
pub struct RawAudio;

impl RawAudio {
    #[allow(dead_code)]
    pub fn from_bytes(_bytes: Vec<u8>, _format: RawAudioFormat) -> Self {
        Self
    }
}

#[allow(dead_code)]
pub enum RawAudioFormat {
    S16LE,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum OutputBackend {
    Pipe,
    Stdout,
}

impl std::fmt::Display for OutputBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OutputBackend::Pipe => write!(f, "pipe"),
            OutputBackend::Stdout => write!(f, "stdout"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::upper_case_acronyms, dead_code)]
pub enum ShairportAudioFormat {
    S16LE,
    S24LE,
    S32LE,
    F32LE,
}

impl std::fmt::Display for ShairportAudioFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShairportAudioFormat::S16LE => write!(f, "S16"), // simplified for libconfig
            ShairportAudioFormat::S24LE => write!(f, "S24"),
            ShairportAudioFormat::S32LE => write!(f, "S32"),
            ShairportAudioFormat::F32LE => write!(f, "F32"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ShairportConfig {
    pub name: String,
    pub port: u16,
    pub password: Option<String>,
    pub pipe_path: PathBuf,
    pub metadata_pipe_path: Option<PathBuf>,
    pub output_backend: OutputBackend,
    #[allow(dead_code)]
    pub audio_format: ShairportAudioFormat,
    pub airplay2_enabled: bool,
    pub interface: Option<String>,
    pub log_verbosity: u8,
    pub udp_port_base: u16,
}

impl Default for ShairportConfig {
    fn default() -> Self {
        use rand::Rng;
        let random_suffix: u32 = rand::thread_rng().r#gen();
        Self {
            name: format!("test-receiver-{random_suffix}"),
            port: 5000,
            password: None,
            pipe_path: PathBuf::from(format!("/tmp/shairport_pipe_{random_suffix}")),
            metadata_pipe_path: None,
            output_backend: OutputBackend::Pipe,
            audio_format: ShairportAudioFormat::S16LE,
            airplay2_enabled: false,
            interface: Some("lo".to_string()),
            log_verbosity: 2,
            udp_port_base: 6000,
        }
    }
}

impl ShairportConfig {
    pub async fn generate_config_file(&self) -> io::Result<PathBuf> {
        let config_dir = PathBuf::from("target/shairport-sync/configs");
        fs::create_dir_all(&config_dir).await?;

        let config_path = config_dir.join(format!("{}.conf", self.name));

        let password_line = self
            .password
            .as_ref()
            .map_or_else(String::new, |p| format!("password = \"{p}\";"));

        let interface_line = self
            .interface
            .as_ref()
            .map_or_else(String::new, |i| format!("interface = \"{i}\";"));

        let metadata_enabled = if self.metadata_pipe_path.is_some() {
            "yes"
        } else {
            "no"
        };
        let metadata_pipe_line = self
            .metadata_pipe_path
            .as_ref()
            .map_or_else(String::new, |p| format!("pipe_name = \"{}\";", p.display()));

        let alsa_line = if self.output_backend == OutputBackend::Pipe {
            String::new()
        } else {
            // For stdout backend, output_format is sometimes set inside 'alsa' section
            // or 'stdout' section depending on shairport version.
            String::new()
        };

        let config_content = format!(
            r#"
general = {{
    name = "{name}";
    port = {port};
    {password_line}
    output_backend = "{backend}";
    mdns_backend = "avahi";
    interpolation = "basic";
    {interface_line}
}};

sessioncontrol = {{
    allow_session_interruption = "yes";
}};

pipe = {{
    name = "{pipe_path}";
    audio_backend_buffer_desired_length_in_seconds = 1.0;
}};

{alsa_line}

metadata = {{
    enabled = "{metadata_enabled}";
    {metadata_pipe_line}
}};

diagnostics = {{
    log_verbosity = {log_verbosity};
}};

airplay = {{
    udp_port_base = {udp_port_base};
    udp_port_range = 100;
}};
"#,
            name = self.name,
            port = self.port,
            password_line = password_line,
            backend = self.output_backend,
            interface_line = interface_line,
            pipe_path = self.pipe_path.display(),
            metadata_enabled = metadata_enabled,
            metadata_pipe_line = metadata_pipe_line,
            log_verbosity = self.log_verbosity,
            udp_port_base = self.udp_port_base,
            alsa_line = alsa_line
        );

        fs::write(&config_path, config_content).await?;
        Ok(config_path)
    }
}

#[allow(dead_code)]
pub struct ShairportOutput {
    pub audio_data: Option<Vec<u8>>,
    pub metadata: Option<Vec<u8>>,
    pub logs: Vec<TimestampedLogLine>,
    pub exit_status: Option<ExitStatus>,
}

#[allow(dead_code)]
impl ShairportOutput {
    pub fn to_raw_audio(&self, format: RawAudioFormat) -> Result<RawAudio, AudioError> {
        let audio_data = self
            .audio_data
            .clone()
            .ok_or_else(|| AudioError::InvalidData("No audio data captured".to_string()))?;

        Ok(RawAudio::from_bytes(audio_data, format))
    }

    pub fn verify_audio_received(&self) -> Result<(), ShairportError> {
        let data = self
            .audio_data
            .as_ref()
            .ok_or(ShairportError::NoAudioReceived)?;
        if data.is_empty() {
            Err(ShairportError::NoAudioReceived)
        } else {
            Ok(())
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, thiserror::Error)]
pub enum ShairportError {
    #[error("Subprocess error: {0}")]
    Subprocess(#[from] SubprocessError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("No audio data received")]
    NoAudioReceived,
}

pub struct ShairportSync {
    handle: SubprocessHandle,
    config: ShairportConfig,
    config_file_path: PathBuf,
    audio_capture: Option<(JoinHandle<Vec<u8>>, tokio::sync::oneshot::Sender<()>)>,
}

impl ShairportSync {
    pub async fn start(mut config: ShairportConfig) -> Result<Self, ShairportError> {
        // Reserve ports
        let ports = reserve_ports(2)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        let ports_vec = ports.ports;
        config.port = ports_vec[0];
        config.udp_port_base = ports_vec[1];

        // Generate config file
        let config_file_path = config.generate_config_file().await?;

        let mut audio_capture = None;

        if config.output_backend == OutputBackend::Pipe {
            // Remove pipe if it exists
            if config.pipe_path.exists() {
                fs::remove_file(&config.pipe_path).await?;
            }

            // Create named pipe
            #[cfg(unix)]
            {
                use std::ffi::CString;
                let c_path = CString::new(config.pipe_path.to_str().unwrap()).unwrap();
                unsafe {
                    nix::libc::mkfifo(c_path.as_ptr(), 0o666);
                }
            }
            #[cfg(windows)]
            {
                // Fallback to creating an empty file on Windows
                fs::write(&config.pipe_path, "").await?;
            }

            // Start reader task
            audio_capture = Some(start_pipe_reader(&config.pipe_path).await);
        }

        let shairport_bin = std::env::current_dir()
            .unwrap()
            .join("../../target/shairport-sync/bin/shairport-sync");

        let shairport_bin_str = shairport_bin.to_string_lossy().to_string();

        let binary_path = if shairport_bin.exists() {
            shairport_bin_str
        } else {
            // Fallback to expecting it in PATH if not built locally
            "shairport-sync".to_string()
        };

        let sub_config = SubprocessConfig {
            command: binary_path,
            args: vec![
                "-c".to_string(),
                config_file_path.to_string_lossy().to_string(),
                "--log-to-stderr".to_string(),
            ],
            working_dir: None,
            env_vars: HashMap::new(),
            ready_strategy: ReadyStrategy::LogPattern("Listening for service".to_string()),
            ready_timeout: Duration::from_secs(15),
            #[cfg(unix)]
            graceful_shutdown_signal: nix::sys::signal::Signal::SIGTERM,
            #[cfg(windows)]
            graceful_shutdown_signal: crate::common::subprocess::Signal::SIGTERM,
            shutdown_timeout: Duration::from_secs(5),
            log_prefix: format!("[{}]", config.name),
            post_ready_delay: None,
            max_log_lines: 10000,
        };

        let mut handle = match SubprocessHandle::spawn(sub_config).await {
            Ok(h) => h,
            Err(e) => {
                // Cleanup config and pipe on failure
                let _ = fs::remove_file(&config_file_path).await;
                if config.output_backend == OutputBackend::Pipe {
                    let _ = fs::remove_file(&config.pipe_path).await;
                }
                return Err(e.into());
            }
        };

        handle.ports = ports_vec;

        Ok(Self {
            handle,
            config,
            config_file_path,
            audio_capture,
        })
    }

    pub async fn stop(mut self) -> Result<ShairportOutput, ShairportError> {
        let _logs = self.handle.logs();

        let sub_output = self.handle.stop().await?;

        let mut audio_data = None;

        if let Some((handle, stop_tx)) = self.audio_capture.take() {
            let _ = stop_tx.send(());
            audio_data = handle.await.ok();
        }

        // Cleanup
        let _ = fs::remove_file(&self.config_file_path).await;
        if self.config.output_backend == OutputBackend::Pipe {
            let _ = fs::remove_file(&self.config.pipe_path).await;
        }

        Ok(ShairportOutput {
            audio_data,
            metadata: None,
            logs: sub_output.logs,
            exit_status: sub_output.exit_status,
        })
    }

    pub fn device_config(&self) -> AirPlayDevice {
        let mut caps = DeviceCapabilities::default();
        if self.config.airplay2_enabled {
            caps.airplay2 = true;
        }

        let raop_caps = RaopCapabilities {
            codecs: vec![RaopCodec::Pcm, RaopCodec::Alac],
            encryption_types: vec![RaopEncryption::Rsa, RaopEncryption::None],
            ..Default::default()
        };

        AirPlayDevice {
            id: self.config.name.clone(),
            name: self.config.name.clone(),
            model: None,
            addresses: vec!["127.0.0.1".parse().unwrap()],
            port: self.config.port,
            capabilities: caps,
            raop_port: Some(self.config.port),
            raop_capabilities: Some(raop_caps),
            txt_records: HashMap::new(),
            last_seen: None,
        }
    }
}

pub async fn start_pipe_reader(
    pipe_path: &Path,
) -> (JoinHandle<Vec<u8>>, tokio::sync::oneshot::Sender<()>) {
    let (stop_tx, mut stop_rx) = tokio::sync::oneshot::channel();
    let pipe_path = pipe_path.to_path_buf();

    let handle = tokio::spawn(async move {
        let mut data = Vec::new();
        // Wait for the pipe to be available, or stop signal
        let file = loop {
            tokio::select! {
                _ = &mut stop_rx => return data,
                res = fs::File::open(&pipe_path) => {
                    if let Ok(f) = res {
                        break f;
                    }
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            }
        };

        use tokio::io::AsyncReadExt;
        let mut reader = tokio::io::BufReader::new(file);
        let mut buf = [0u8; 4096];

        loop {
            tokio::select! {
                _ = &mut stop_rx => break,
                res = reader.read(&mut buf) => {
                    match res {
                        Ok(0) => break, // EOF
                        Ok(n) => {
                            data.extend_from_slice(&buf[..n]);
                            if data.len() > 100 * 1024 * 1024 {
                                tracing::warn!("Pipe reader reached 100MB limit, stopping.");
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            }
        }
        data
    });

    (handle, stop_tx)
}
