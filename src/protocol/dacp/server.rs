//! DACP HTTP server for receiving commands

use std::sync::Arc;

use super::commands::{CommandResult, DacpCommand};
use super::service::DacpError;

/// Handler trait for DACP commands
pub trait DacpHandler: Send + Sync {
    /// Handle incoming command
    fn handle_command(&self, command: DacpCommand) -> CommandResult;

    /// Verify Active-Remote token
    fn verify_token(&self, token: &str) -> bool;
}

/// DACP HTTP server
pub struct DacpServer<H: DacpHandler> {
    /// Handler for commands
    handler: Arc<H>,
    /// Expected Active-Remote token
    #[allow(dead_code)]
    expected_token: String,
    /// Server port
    #[allow(dead_code)]
    port: u16,
    /// Running state
    running: bool,
}

impl<H: DacpHandler + 'static> DacpServer<H> {
    /// Create new DACP server
    #[must_use]
    pub fn new(handler: H, expected_token: String, port: u16) -> Self {
        Self {
            handler: Arc::new(handler),
            expected_token,
            port,
            running: false,
        }
    }

    /// Start the HTTP server
    ///
    /// # Errors
    ///
    /// Returns error if server fails to start
    #[allow(clippy::unused_async)]
    pub async fn start(&mut self) -> Result<(), DacpError> {
        // HTTP server implementation using hyper or similar
        // Listen for GET requests on /ctrl-int/1/*

        self.running = true;

        // Pseudo-code for request handling:
        // loop {
        //     let request = accept_connection().await;
        //     let token = request.headers.get("Active-Remote");
        //
        //     if !self.handler.verify_token(token) {
        //         respond_403();
        //         continue;
        //     }
        //
        //     let command = DacpCommand::from_path(request.path());
        //     let result = self.handler.handle_command(command);
        //
        //     respond_204();
        // }

        Ok(())
    }

    /// Stop the server
    #[allow(clippy::unused_async)]
    pub async fn stop(&mut self) {
        self.running = false;
    }

    /// Check if server is running
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// Process an HTTP request (for testing)
    #[must_use]
    pub fn process_request(
        &self,
        method: &str,
        path: &str,
        active_remote: Option<&str>,
    ) -> HttpResponse {
        // Only accept GET requests
        if method != "GET" {
            return HttpResponse::method_not_allowed();
        }

        // Verify Active-Remote token
        match active_remote {
            Some(token) if self.handler.verify_token(token) => {}
            _ => return HttpResponse::forbidden(),
        }

        // Parse command
        let Some(command) = DacpCommand::from_path(path) else {
            return HttpResponse::not_found();
        };

        // Execute command
        let result = self.handler.handle_command(command);

        match result {
            CommandResult::Success => HttpResponse::no_content(),
            CommandResult::NotSupported => HttpResponse::not_implemented(),
            CommandResult::Failed(_) => HttpResponse::internal_error(),
        }
    }
}

/// Simple HTTP response representation
pub struct HttpResponse {
    pub status: u16,
    pub reason: &'static str,
}

impl HttpResponse {
    #[must_use]
    pub fn no_content() -> Self {
        Self {
            status: 204,
            reason: "No Content",
        }
    }

    #[must_use]
    pub fn forbidden() -> Self {
        Self {
            status: 403,
            reason: "Forbidden",
        }
    }

    #[must_use]
    pub fn not_found() -> Self {
        Self {
            status: 404,
            reason: "Not Found",
        }
    }

    #[must_use]
    pub fn method_not_allowed() -> Self {
        Self {
            status: 405,
            reason: "Method Not Allowed",
        }
    }

    #[must_use]
    pub fn not_implemented() -> Self {
        Self {
            status: 501,
            reason: "Not Implemented",
        }
    }

    #[must_use]
    pub fn internal_error() -> Self {
        Self {
            status: 500,
            reason: "Internal Server Error",
        }
    }
}

/// Default command handler that forwards to callbacks
pub struct CallbackHandler {
    token: String,
    on_play: Option<Box<dyn Fn() + Send + Sync>>,
    on_pause: Option<Box<dyn Fn() + Send + Sync>>,
    on_next: Option<Box<dyn Fn() + Send + Sync>>,
    on_previous: Option<Box<dyn Fn() + Send + Sync>>,
    on_volume_up: Option<Box<dyn Fn() + Send + Sync>>,
    on_volume_down: Option<Box<dyn Fn() + Send + Sync>>,
}

impl CallbackHandler {
    /// Create new callback handler
    #[must_use]
    pub fn new(token: String) -> Self {
        Self {
            token,
            on_play: None,
            on_pause: None,
            on_next: None,
            on_previous: None,
            on_volume_up: None,
            on_volume_down: None,
        }
    }

    /// Set play callback
    #[must_use]
    pub fn on_play<F: Fn() + Send + Sync + 'static>(mut self, f: F) -> Self {
        self.on_play = Some(Box::new(f));
        self
    }

    /// Set pause callback
    #[must_use]
    pub fn on_pause<F: Fn() + Send + Sync + 'static>(mut self, f: F) -> Self {
        self.on_pause = Some(Box::new(f));
        self
    }

    /// Set next track callback
    #[must_use]
    pub fn on_next<F: Fn() + Send + Sync + 'static>(mut self, f: F) -> Self {
        self.on_next = Some(Box::new(f));
        self
    }

    /// Set previous track callback
    #[must_use]
    pub fn on_previous<F: Fn() + Send + Sync + 'static>(mut self, f: F) -> Self {
        self.on_previous = Some(Box::new(f));
        self
    }
}

impl DacpHandler for CallbackHandler {
    fn handle_command(&self, command: DacpCommand) -> CommandResult {
        match command {
            DacpCommand::Play | DacpCommand::PlayResume | DacpCommand::PlayResume2 => {
                if let Some(ref f) = self.on_play {
                    f();
                    CommandResult::Success
                } else {
                    CommandResult::NotSupported
                }
            }
            DacpCommand::Pause => {
                if let Some(ref f) = self.on_pause {
                    f();
                    CommandResult::Success
                } else {
                    CommandResult::NotSupported
                }
            }
            DacpCommand::PlayPause => {
                // Toggle - implementation depends on current state
                CommandResult::Success
            }
            DacpCommand::NextItem => {
                if let Some(ref f) = self.on_next {
                    f();
                    CommandResult::Success
                } else {
                    CommandResult::NotSupported
                }
            }
            DacpCommand::PrevItem => {
                if let Some(ref f) = self.on_previous {
                    f();
                    CommandResult::Success
                } else {
                    CommandResult::NotSupported
                }
            }
            DacpCommand::VolumeUp => {
                if let Some(ref f) = self.on_volume_up {
                    f();
                    CommandResult::Success
                } else {
                    CommandResult::NotSupported
                }
            }
            DacpCommand::VolumeDown => {
                if let Some(ref f) = self.on_volume_down {
                    f();
                    CommandResult::Success
                } else {
                    CommandResult::NotSupported
                }
            }
            _ => CommandResult::NotSupported,
        }
    }

    fn verify_token(&self, token: &str) -> bool {
        token == self.token
    }
}
