use std::sync::atomic::{AtomicBool, Ordering};

use crate::protocol::dacp::commands::{CommandResult, DacpCommand};
use crate::protocol::dacp::server::{DacpHandler, DacpServer};
use crate::protocol::dacp::service::DacpServiceConfig;

struct TestHandler {
    token: String,
    play_called: AtomicBool,
}

impl DacpHandler for TestHandler {
    fn handle_command(&self, command: DacpCommand) -> CommandResult {
        match command {
            DacpCommand::Play => {
                self.play_called.store(true, Ordering::SeqCst);
                CommandResult::Success
            }
            _ => CommandResult::NotSupported,
        }
    }

    fn verify_token(&self, token: &str) -> bool {
        token == self.token
    }
}

#[test]
fn test_process_request_success() {
    let handler = TestHandler {
        token: "12345".to_string(),
        play_called: AtomicBool::new(false),
    };

    let server = DacpServer::new(handler, "12345".to_string(), 3689);

    let response = server.process_request("GET", "/ctrl-int/1/play", Some("12345"));

    assert_eq!(response.status, 204);
}

#[test]
fn test_process_request_bad_token() {
    let handler = TestHandler {
        token: "12345".to_string(),
        play_called: AtomicBool::new(false),
    };

    let server = DacpServer::new(handler, "12345".to_string(), 3689);

    let response = server.process_request("GET", "/ctrl-int/1/play", Some("wrong"));

    assert_eq!(response.status, 403);
}

#[test]
fn test_process_request_unknown_command() {
    let handler = TestHandler {
        token: "12345".to_string(),
        play_called: AtomicBool::new(false),
    };

    let server = DacpServer::new(handler, "12345".to_string(), 3689);

    let response = server.process_request("GET", "/ctrl-int/1/unknown", Some("12345"));

    assert_eq!(response.status, 404);
}

#[test]
fn test_service_config() {
    let config = DacpServiceConfig::new();

    assert!(!config.dacp_id.is_empty());
    assert!(!config.active_remote.is_empty());
    assert!(config.instance_name().starts_with("iTunes_Ctrl_"));
}
