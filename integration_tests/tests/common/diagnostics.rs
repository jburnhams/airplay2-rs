use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::common::subprocess::TimestampedLogLine;

#[allow(dead_code)]
pub fn save_test_logs(
    test_name: &str,
    logs: &[TimestampedLogLine],
    extra_files: &[(&str, &[u8])],
) -> PathBuf {
    let mut target_dir = match std::env::current_dir() {
        Ok(pb) => pb,
        Err(_) => PathBuf::from("."),
    };

    if !target_dir.join("target").exists()
        && target_dir
            .parent()
            .map(|p| p.join("target").exists())
            .unwrap_or(false)
    {
        target_dir = target_dir.parent().unwrap().to_path_buf();
    }

    let timestamp = chrono::Utc::now().timestamp_millis();
    let log_dir = target_dir
        .join("target")
        .join("integration-tests")
        .join(test_name)
        .join(timestamp.to_string());

    if !log_dir.exists() {
        let _ = fs::create_dir_all(&log_dir);
    }

    let log_content = logs
        .iter()
        .map(format_log_line)
        .collect::<Vec<_>>()
        .join("\n");
    let log_file_path = log_dir.join("test.log");
    let _ = fs::write(&log_file_path, log_content);

    for (filename, content) in extra_files {
        let _ = fs::write(log_dir.join(filename), content);
    }

    log_file_path
}

#[allow(dead_code)]
pub fn format_log_line(line: &TimestampedLogLine) -> String {
    let elapsed = line.timestamp.elapsed();
    format!(
        "[+{}ms] [{}] {}",
        elapsed.as_millis(),
        line.stream,
        line.line
    )
}

#[allow(dead_code)]
pub struct TestDiagnostics {
    pub test_name: String,
    pub subprocess_logs: HashMap<String, Vec<TimestampedLogLine>>,
    pub audio_files: Vec<(String, Vec<u8>)>,
    pub rtp_captures: Vec<(String, Vec<u8>)>,
}

#[allow(dead_code)]
impl TestDiagnostics {
    pub fn new(test_name: &str) -> Self {
        Self {
            test_name: test_name.to_string(),
            subprocess_logs: HashMap::new(),
            audio_files: Vec::new(),
            rtp_captures: Vec::new(),
        }
    }

    pub fn save(&self) -> PathBuf {
        let mut extra_files = Vec::new();
        for (filename, content) in &self.audio_files {
            extra_files.push((filename.as_str(), content.as_slice()));
        }
        for (filename, content) in &self.rtp_captures {
            extra_files.push((filename.as_str(), content.as_slice()));
        }

        let mut all_logs = Vec::new();
        for (process, logs) in &self.subprocess_logs {
            for log in logs {
                let mut l = log.clone();
                l.line = format!("[{}] {}", process, l.line);
                all_logs.push(l);
            }
        }
        all_logs.sort_by_key(|a| a.timestamp);

        save_test_logs(&self.test_name, &all_logs, &extra_files)
    }
}
