#![allow(dead_code)]
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::time::Duration;

use airplay2::types::RaopCapabilities;
use airplay2::{AirPlayDevice, DeviceCapabilities};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use crate::common::subprocess::{
    ReadyStrategy, SubprocessConfig, SubprocessError, SubprocessHandle, TimestampedLogLine,
};

#[derive(Debug, Clone, PartialEq, Eq)]
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

pub struct ShairportOutput {
    pub audio_data: Option<Vec<u8>>,
    pub metadata: Option<Vec<u8>>,
    pub logs: Vec<TimestampedLogLine>,
    pub exit_status: Option<ExitStatus>,
}

#[derive(Debug, thiserror::Error)]
pub enum ShairportError {
    #[error("Subprocess error: {0}")]
    Subprocess(#[from] SubprocessError),
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("Failed to parse port: {0}")]
    ParsePort(String),
}

pub struct ShairportSync {
    handle: SubprocessHandle,
    pub config: ShairportConfig,
    config_file_path: PathBuf,
    audio_capture: Option<JoinHandle<Vec<u8>>>,
    audio_capture_cancel: Option<oneshot::Sender<()>>,
}

impl ShairportSync {
    pub async fn start(config: ShairportConfig) -> Result<Self, ShairportError> {
        let config_file_path = config.generate_config_file()?;

        // Create named pipe if using Pipe backend
        if config.output_backend == OutputBackend::Pipe {
            let pipe_dir = config.pipe_path.parent().unwrap();
            fs::create_dir_all(pipe_dir)?;
            if config.pipe_path.exists() {
                fs::remove_file(&config.pipe_path)?;
            }

            #[cfg(unix)]
            {
                let path_str = config.pipe_path.to_string_lossy().to_string();
                std::process::Command::new("mkfifo")
                    .arg(&path_str)
                    .output()
                    .map_err(|e| {
                        io::Error::new(io::ErrorKind::Other, format!("Failed to run mkfifo: {}", e))
                    })?;
            }
        }

        let binary_path = PathBuf::from("target/shairport-sync/bin/shairport-sync");
        if !binary_path.exists() {
            return Err(ShairportError::Io(io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "shairport-sync binary not found at {}",
                    binary_path.display()
                ),
            )));
        }

        let sub_config = SubprocessConfig {
            command: binary_path.to_string_lossy().to_string(),
            args: vec![
                "-c".to_string(),
                config_file_path.to_string_lossy().to_string(),
                "--log-to-stderr".to_string(),
            ],
            working_dir: None,
            env_vars: Default::default(),
            ready_strategy: ReadyStrategy::LogPattern("Listening for MAC address".to_string()),
            ready_timeout: Duration::from_secs(15),
            log_prefix: format!("[{}]", config.name),
            ..Default::default()
        };

        let handle = SubprocessHandle::spawn(sub_config).await?;

        let (audio_capture, audio_capture_cancel) = if config.output_backend == OutputBackend::Pipe
        {
            let (tx, rx) = oneshot::channel();
            let pipe_path = config.pipe_path.clone();
            let jh = tokio::spawn(async move { start_pipe_reader(&pipe_path, rx).await });
            (Some(jh), Some(tx))
        } else {
            (None, None)
        };

        Ok(Self {
            handle,
            config,
            config_file_path,
            audio_capture,
            audio_capture_cancel,
        })
    }

    pub async fn stop(mut self) -> Result<ShairportOutput, ShairportError> {
        if let Some(cancel) = self.audio_capture_cancel.take() {
            let _ = cancel.send(());
        }

        let output = self.handle.stop().await?;

        let audio_data = if let Some(jh) = self.audio_capture {
            jh.await.ok()
        } else {
            None
        };

        // Cleanup
        let _ = fs::remove_file(&self.config_file_path);
        if self.config.output_backend == OutputBackend::Pipe {
            let _ = fs::remove_file(&self.config.pipe_path);
        }
        if let Some(ref metadata_pipe) = self.config.metadata_pipe_path {
            let _ = fs::remove_file(metadata_pipe);
        }

        Ok(ShairportOutput {
            audio_data,
            metadata: None,
            logs: output.logs,
            exit_status: output.exit_status,
        })
    }

    pub fn device_config(&self) -> AirPlayDevice {
        Self::build_device_config(&self.config)
    }

    pub fn build_device_config(config: &ShairportConfig) -> AirPlayDevice {
        use std::collections::HashMap;

        let capabilities = DeviceCapabilities {
            airplay2: config.airplay2_enabled,
            supports_transient_pairing: true,
            ..Default::default()
        };

        AirPlayDevice {
            id: config.name.clone(),
            name: config.name.clone(),
            model: Some("ShairportSync".to_string()),
            addresses: vec!["127.0.0.1".parse().unwrap()],
            port: config.port,
            capabilities,
            raop_port: Some(config.port),
            raop_capabilities: Some(RaopCapabilities {
                codecs: vec![
                    airplay2::types::RaopCodec::Pcm,
                    airplay2::types::RaopCodec::Alac,
                ],
                encryption_types: vec![
                    airplay2::types::RaopEncryption::Rsa,
                    airplay2::types::RaopEncryption::None,
                ],
                ..Default::default()
            }),
            txt_records: HashMap::new(),
            last_seen: None,
        }
    }
}

pub async fn start_pipe_reader(pipe_path: &Path, mut cancel: oneshot::Receiver<()>) -> Vec<u8> {
    use tokio::io::AsyncReadExt;

    let mut data = Vec::new();

    let file_result = loop {
        tokio::select! {
            _ = &mut cancel => return data,
            _ = tokio::time::sleep(Duration::from_millis(100)) => {}
        }

        if !pipe_path.exists() {
            continue;
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            match std::fs::OpenOptions::new()
                .read(true)
                .custom_flags(nix::libc::O_NONBLOCK)
                .open(pipe_path)
            {
                Ok(f) => {
                    use std::os::fd::AsRawFd;
                    unsafe {
                        let flags = nix::libc::fcntl(f.as_raw_fd(), nix::libc::F_GETFL);
                        nix::libc::fcntl(
                            f.as_raw_fd(),
                            nix::libc::F_SETFL,
                            flags & !nix::libc::O_NONBLOCK,
                        );
                    }
                    break Ok(tokio::fs::File::from_std(f));
                }
                Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
                Err(e) if e.raw_os_error() == Some(nix::libc::ENXIO) => {
                    continue; // Writer not open yet
                }
                Err(e) => break Err(e),
            }
        }

        #[cfg(not(unix))]
        {
            match tokio::fs::File::open(pipe_path).await {
                Ok(f) => break Ok(f),
                Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
                Err(e) => break Err(e),
            }
        }
    };

    if let Ok(mut f) = file_result {
        let mut buf = [0u8; 4096];
        loop {
            tokio::select! {
                res = f.read(&mut buf) => {
                    match res {
                        Ok(0) => break, // EOF
                        Ok(n) => data.extend_from_slice(&buf[..n]),
                        Err(_) => break, // Error reading
                    }
                }
                _ = &mut cancel => {
                    break;
                }
            }
        }
    }

    data
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShairportAudioFormat {
    S16LE,
    S24LE,
    S32LE,
    F32LE,
}

impl std::fmt::Display for ShairportAudioFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShairportAudioFormat::S16LE => write!(f, "S16LE"),
            ShairportAudioFormat::S24LE => write!(f, "S24LE"),
            ShairportAudioFormat::S32LE => write!(f, "S32LE"),
            ShairportAudioFormat::F32LE => write!(f, "F32LE"),
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
    pub audio_format: ShairportAudioFormat,
    pub airplay2_enabled: bool,
    pub interface: Option<String>,
    pub log_verbosity: u8,
    pub udp_port_base: u16,
}

impl ShairportConfig {
    pub fn generate_config_file(&self) -> io::Result<PathBuf> {
        let config_dir = PathBuf::from("target/shairport-sync/configs");
        fs::create_dir_all(&config_dir)?;

        let config_path = config_dir.join(format!("{}.conf", self.name));
        let mut file = fs::File::create(&config_path)?;

        let password_line = match &self.password {
            Some(pw) => format!("password = \"{}\";", pw),
            None => "".to_string(),
        };

        let interface_line = match &self.interface {
            Some(iface) => format!("interface = \"{}\";", iface),
            None => "".to_string(),
        };

        let metadata_enabled = if self.metadata_pipe_path.is_some() {
            "yes"
        } else {
            "no"
        };
        let metadata_pipe_line = match &self.metadata_pipe_path {
            Some(path) => format!(
                "include_cover_art = \"no\";\n    pipe_name = \"{}\";",
                path.display()
            ),
            None => "".to_string(),
        };

        let content = format!(
            "general = {{\n\tname = \"{name}\";\n\tport = \
             {port};\n\t{password_line}\n\toutput_backend = \"{backend}\";\n\tmdns_backend = \
             \"avahi\";\n\tinterpolation = \"basic\";\n\t{interface_line}\n}};\n\nsessioncontrol \
             = {{\n\tallow_session_interruption = \"yes\";\n}};\n\npipe = {{\n\tname = \
             \"{pipe_path}\";\n\taudio_backend_buffer_desired_length_in_seconds = \
             1.0;\n}};\n\nmetadata = {{\n\tenabled = \
             \"{metadata_enabled}\";\n\t{metadata_pipe_line}\n}};\n\ndiagnostics = \
             {{\n\tlog_verbosity = {log_verbosity};\n}};\n\nairplay = {{\n\tudp_port_base = \
             {udp_port_base};\n\tudp_port_range = 100;\n}};\n",
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
        );

        file.write_all(content.as_bytes())?;
        Ok(config_path)
    }
}
