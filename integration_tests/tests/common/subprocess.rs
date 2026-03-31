use std::collections::HashMap;
use std::future::Future;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::pin::Pin;
use std::process::ExitStatus;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[cfg(unix)]
use nix::sys::signal::Signal;

/// Graceful shutdown signal type.
/// On Unix this is the real nix Signal; on Windows it's a stub (the value is
/// never passed to kill() — see the `#[cfg(windows)]` block in `stop()`).
#[cfg(windows)]
#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(
    non_camel_case_types,
    dead_code,
    reason = "Windows dummy types might not be fully used or conform to standard casing"
)]
pub enum Signal {
    SIGTERM,
    SIGKILL,
}

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::TcpStream;
use tokio::process::{Child, Command};
use tokio::time::{Instant, sleep};

#[derive(Clone, Debug, PartialEq)]
pub enum LogStream {
    Stdout,
    Stderr,
}

impl std::fmt::Display for LogStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogStream::Stdout => write!(f, "STDOUT"),
            LogStream::Stderr => write!(f, "STDERR"),
        }
    }
}

#[derive(Clone, Debug)]
#[allow(dead_code, reason = "Used in some test modules but not all")]
pub struct TimestampedLogLine {
    pub timestamp: std::time::Instant,
    pub stream: LogStream,
    pub line: String,
}

#[allow(dead_code, reason = "Used in some test modules but not all")]
pub enum ReadyStrategy {
    LogPattern(String),
    TcpPort(u16),
    Delay(Duration),
    #[allow(
        clippy::type_complexity,
        reason = "Custom strategy needs complex callback signature"
    )]
    Custom(Box<dyn Fn() -> Pin<Box<dyn Future<Output = bool>>> + Send + Sync>),
}

pub struct SubprocessConfig {
    pub command: String,
    pub args: Vec<String>,
    pub working_dir: Option<PathBuf>,
    pub env_vars: HashMap<String, String>,
    pub ready_strategy: ReadyStrategy,
    pub ready_timeout: Duration,
    #[allow(dead_code, reason = "Only used on Unix in the stop() method")]
    pub graceful_shutdown_signal: Signal,
    pub shutdown_timeout: Duration,
    pub log_prefix: String,
    pub post_ready_delay: Option<Duration>,
    pub max_log_lines: usize,
}

impl Default for SubprocessConfig {
    fn default() -> Self {
        Self {
            command: String::new(),
            args: Vec::new(),
            working_dir: None,
            env_vars: HashMap::new(),
            ready_strategy: ReadyStrategy::Delay(Duration::from_millis(0)),
            ready_timeout: Duration::from_secs(15),
            graceful_shutdown_signal: Signal::SIGTERM,
            shutdown_timeout: Duration::from_secs(5),
            log_prefix: "[subprocess]".to_string(),
            post_ready_delay: None,
            max_log_lines: 10000,
        }
    }
}

#[allow(dead_code, reason = "Used in some test modules but not all")]
pub struct SubprocessHandle {
    process: Child,
    config: SubprocessConfig,
    started_at: std::time::Instant,
    log_lines: Arc<Mutex<Vec<TimestampedLogLine>>>,
    pub ports: Vec<u16>,
}

#[allow(dead_code, reason = "Used in some test modules but not all")]
pub struct SubprocessOutput {
    pub exit_status: Option<ExitStatus>,
    pub logs: Vec<TimestampedLogLine>,
    pub duration: Duration,
}

#[allow(dead_code, reason = "Used in some test modules but not all")]
#[derive(Debug, thiserror::Error)]
pub enum SubprocessError {
    #[error("Spawn failed for {command}: {source}")]
    SpawnFailed {
        command: String,
        source: std::io::Error,
    },
    #[error("Ready timeout after {timeout:?}. Stderr tail: {stderr_tail:?}")]
    ReadyTimeout {
        timeout: Duration,
        stderr_tail: Vec<String>,
    },
    #[error("Process exited early with status {status}. Stderr tail: {stderr_tail:?}")]
    EarlyExit {
        status: ExitStatus,
        stderr_tail: Vec<String>,
    },
    #[error("Shutdown failed: {source}")]
    ShutdownFailed { source: std::io::Error },
}

impl SubprocessHandle {
    pub async fn spawn(config: SubprocessConfig) -> Result<Self, SubprocessError> {
        let mut command = Command::new(&config.command);
        command.args(&config.args);

        if let Some(dir) = &config.working_dir {
            command.current_dir(dir);
        }

        command.envs(&config.env_vars);
        command
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);

        let mut process = command.spawn().map_err(|e| SubprocessError::SpawnFailed {
            command: config.command.clone(),
            source: e,
        })?;

        let stdout = process.stdout.take().expect("Failed to capture stdout");
        let stderr = process.stderr.take().expect("Failed to capture stderr");

        let log_lines = Arc::new(Mutex::new(Vec::new()));
        let log_lines_clone = log_lines.clone();
        let log_prefix = config.log_prefix.clone();
        let max_log_lines = config.max_log_lines;
        let started_at = std::time::Instant::now();

        let (ready_tx, mut ready_rx) = tokio::sync::mpsc::channel(1);

        // Task to read stdout
        let log_lines_out = log_lines_clone.clone();
        let ready_pattern_out = match &config.ready_strategy {
            ReadyStrategy::LogPattern(p) => Some(p.clone()),
            _ => None,
        };
        let ready_tx_out = ready_tx.clone();
        let log_prefix_out = log_prefix.clone();

        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                tracing::debug!("{} STDOUT: {}", log_prefix_out, line);
                {
                    let mut logs = log_lines_out.lock().unwrap();
                    if logs.len() < max_log_lines {
                        logs.push(TimestampedLogLine {
                            timestamp: std::time::Instant::now(),
                            stream: LogStream::Stdout,
                            line: line.clone(),
                        });
                    }
                }
                if ready_pattern_out
                    .as_ref()
                    .is_some_and(|pattern| line.contains(pattern))
                {
                    let _ = ready_tx_out.send(()).await;
                }
            }
        });

        // Task to read stderr
        let log_lines_err = log_lines_clone.clone();
        let ready_pattern_err = match &config.ready_strategy {
            ReadyStrategy::LogPattern(p) => Some(p.clone()),
            _ => None,
        };
        let ready_tx_err = ready_tx.clone();
        let log_prefix_err = log_prefix.clone();

        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                tracing::warn!("{} STDERR: {}", log_prefix_err, line);
                {
                    let mut logs = log_lines_err.lock().unwrap();
                    if logs.len() < max_log_lines {
                        logs.push(TimestampedLogLine {
                            timestamp: std::time::Instant::now(),
                            stream: LogStream::Stderr,
                            line: line.clone(),
                        });
                    }
                }
                if ready_pattern_err
                    .as_ref()
                    .is_some_and(|pattern| line.contains(pattern))
                {
                    let _ = ready_tx_err.send(()).await;
                }
            }
        });

        // Wait for ready strategy
        let timeout = config.ready_timeout;
        let start_wait = Instant::now();

        match &config.ready_strategy {
            ReadyStrategy::LogPattern(_) => {
                tokio::select! {
                    _ = ready_rx.recv() => {
                        // Ready pattern found
                    }
                    _ = sleep(timeout) => {
                        let _ = process.kill().await;
                        let stderr_tail = Self::extract_stderr_tail(&log_lines_clone);
                        return Err(SubprocessError::ReadyTimeout { timeout, stderr_tail });
                    }
                    status = process.wait() => {
                        let stderr_tail = Self::extract_stderr_tail(&log_lines_clone);
                        return Err(SubprocessError::EarlyExit {
                            status: status.unwrap_or_else(|_| make_exit_status(1)),
                            stderr_tail
                        });
                    }
                }
            }
            ReadyStrategy::TcpPort(port) => {
                let addr: SocketAddr = format!("127.0.0.1:{}", port).parse().unwrap();
                let mut ready = false;
                while start_wait.elapsed() < timeout {
                    if let Ok(Some(status)) = process.try_wait() {
                        let stderr_tail = Self::extract_stderr_tail(&log_lines_clone);
                        return Err(SubprocessError::EarlyExit {
                            status,
                            stderr_tail,
                        });
                    }

                    if wait_for_tcp_port(addr, Duration::from_millis(100))
                        .await
                        .is_ok()
                    {
                        ready = true;
                        break;
                    }
                }

                if !ready {
                    let _ = process.kill().await;
                    let stderr_tail = Self::extract_stderr_tail(&log_lines_clone);
                    return Err(SubprocessError::ReadyTimeout {
                        timeout,
                        stderr_tail,
                    });
                }
            }
            ReadyStrategy::Delay(d) => {
                let wait_dur = *d;
                tokio::select! {
                    _ = sleep(wait_dur) => {}
                    status = process.wait() => {
                        let stderr_tail = Self::extract_stderr_tail(&log_lines_clone);
                        return Err(SubprocessError::EarlyExit {
                            status: status.unwrap_or_else(|_| make_exit_status(1)),
                            stderr_tail
                        });
                    }
                }
            }
            ReadyStrategy::Custom(f) => {
                let mut ready = false;
                while start_wait.elapsed() < timeout {
                    if let Ok(Some(status)) = process.try_wait() {
                        let stderr_tail = Self::extract_stderr_tail(&log_lines_clone);
                        return Err(SubprocessError::EarlyExit {
                            status,
                            stderr_tail,
                        });
                    }

                    if f().await {
                        ready = true;
                        break;
                    }
                    sleep(Duration::from_millis(100)).await;
                }

                if !ready {
                    let _ = process.kill().await;
                    let stderr_tail = Self::extract_stderr_tail(&log_lines_clone);
                    return Err(SubprocessError::ReadyTimeout {
                        timeout,
                        stderr_tail,
                    });
                }
            }
        }

        if let Some(delay) = config.post_ready_delay {
            sleep(delay).await;
        }

        Ok(Self {
            process,
            config,
            started_at,
            log_lines,
            ports: Vec::new(),
        })
    }

    pub async fn stop(mut self) -> Result<SubprocessOutput, SubprocessError> {
        #[cfg(unix)]
        {
            use nix::sys::signal::kill;
            use nix::unistd::Pid;
            if let Some(id) = self.process.id() {
                let pid = Pid::from_raw(id as i32);
                let _ = kill(pid, self.config.graceful_shutdown_signal);
            }
        }

        #[cfg(windows)]
        {
            let _ = self.process.kill().await;
        }

        let exit_status =
            match tokio::time::timeout(self.config.shutdown_timeout, self.process.wait()).await {
                Ok(Ok(status)) => Some(status),
                _ => {
                    let _ = self.process.kill().await;
                    self.process.wait().await.ok()
                }
            };

        let logs = self.log_lines.lock().unwrap().clone();

        Ok(SubprocessOutput {
            exit_status,
            logs,
            duration: self.started_at.elapsed(),
        })
    }

    #[allow(dead_code, reason = "Used in some test modules but not all")]
    pub fn pid(&self) -> Option<u32> {
        self.process.id()
    }

    #[allow(dead_code, reason = "Used in some test modules but not all")]
    pub fn elapsed(&self) -> Duration {
        self.started_at.elapsed()
    }

    pub fn logs(&self) -> Vec<TimestampedLogLine> {
        self.log_lines.lock().unwrap().clone()
    }

    #[allow(dead_code, reason = "Used in some test modules but not all")]
    pub async fn is_running(&mut self) -> bool {
        match self.process.try_wait() {
            Ok(Some(_)) => false,
            Ok(None) => true,
            Err(_) => false,
        }
    }

    fn extract_stderr_tail(log_lines: &Arc<Mutex<Vec<TimestampedLogLine>>>) -> Vec<String> {
        let logs = log_lines.lock().unwrap();
        logs.iter()
            .filter(|l| l.stream == LogStream::Stderr)
            .rev()
            .take(20)
            .map(|l| l.line.clone())
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }
}

impl Drop for SubprocessHandle {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            use nix::sys::signal::{Signal, kill};
            use nix::unistd::Pid;
            if let Some(id) = self.process.id() {
                let pid = Pid::from_raw(id as i32);
                let _ = kill(pid, Signal::SIGKILL);
            }
        }
        #[cfg(windows)]
        {
            let _ = self.process.start_kill();
        }
    }
}

pub async fn wait_for_tcp_port(addr: SocketAddr, timeout: Duration) -> Result<(), SubprocessError> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if TcpStream::connect(addr).await.is_ok() {
            return Ok(());
        }
        sleep(Duration::from_millis(50)).await;
    }
    Err(SubprocessError::ReadyTimeout {
        timeout,
        stderr_tail: vec!["Timeout waiting for TCP port".to_string()],
    })
}

#[allow(dead_code, reason = "Used in some test modules but not all")]
pub async fn wait_for_port_bound(port: u16, timeout: Duration) -> Result<(), SubprocessError> {
    let addr: SocketAddr = format!("127.0.0.1:{}", port).parse().unwrap();
    wait_for_tcp_port(addr, timeout).await
}

/// Create a failure `ExitStatus` in a cross-platform way.
/// Used as a fallback when `process.wait()` returns an error.
fn make_exit_status(_code: i32) -> std::process::ExitStatus {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        std::process::ExitStatus::from_raw(_code)
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::ExitStatusExt;
        std::process::ExitStatus::from_raw(_code as u32)
    }
}
