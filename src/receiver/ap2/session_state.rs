//! AirPlay 2 Receiver Session State Machine
//!
//! AirPlay 2 sessions have more states than AirPlay 1 due to
//! multi-phase setup and encrypted control channels.

/// Session state for `AirPlay` 2 receiver
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ap2SessionState {
    /// Initial state - TCP connected, awaiting requests
    Connected,

    /// /info requested - sender is querying capabilities
    InfoExchanged,

    /// Pair-setup in progress (SRP exchange)
    PairingSetup {
        /// Current step in SRP protocol (1-4)
        step: u8,
    },

    /// Pair-verify in progress
    PairingVerify {
        /// Current step in verify protocol (1-4)
        step: u8,
    },

    /// Pairing complete - control channel now encrypted
    Paired,

    /// First SETUP complete (event + timing channels)
    SetupPhase1,

    /// Second SETUP complete (audio channels)
    SetupPhase2,

    /// RECORD received - streaming active
    Streaming,

    /// Paused (audio stopped but session alive)
    Paused,

    /// Session ending
    Teardown,

    /// Error state
    Error {
        /// Error code
        code: u32,
        /// Error message
        message: String,
    },
}

impl Ap2SessionState {
    /// Check if this state allows the given RTSP method
    #[must_use]
    pub fn allows_method(&self, method: &str) -> bool {
        match self {
            Self::Connected => matches!(method, "OPTIONS" | "GET" | "POST"),
            Self::InfoExchanged => matches!(method, "OPTIONS" | "GET" | "POST"),
            Self::PairingSetup { .. } => matches!(method, "OPTIONS" | "POST"),
            Self::PairingVerify { .. } => matches!(method, "OPTIONS" | "POST"),
            Self::Paired => matches!(
                method,
                "OPTIONS" | "GET" | "POST" | "SETUP" | "GET_PARAMETER" | "SET_PARAMETER"
            ),
            Self::SetupPhase1 => matches!(
                method,
                "OPTIONS" | "SETUP" | "GET_PARAMETER" | "SET_PARAMETER" | "TEARDOWN"
            ),
            Self::SetupPhase2 => matches!(
                method,
                "OPTIONS" | "RECORD" | "GET_PARAMETER" | "SET_PARAMETER" | "TEARDOWN"
            ),
            Self::Streaming => matches!(
                method,
                "OPTIONS" | "GET_PARAMETER" | "SET_PARAMETER" | "FLUSH" | "TEARDOWN" | "POST"
            ),
            Self::Paused => matches!(
                method,
                "OPTIONS" | "RECORD" | "GET_PARAMETER" | "SET_PARAMETER" | "TEARDOWN"
            ),
            Self::Teardown => matches!(method, "OPTIONS"),
            Self::Error { .. } => false,
        }
    }

    /// Check if the session is in an authenticated state
    #[must_use]
    pub fn is_authenticated(&self) -> bool {
        matches!(
            self,
            Self::Paired | Self::SetupPhase1 | Self::SetupPhase2 | Self::Streaming | Self::Paused
        )
    }

    /// Check if the session is actively streaming
    #[must_use]
    pub fn is_streaming(&self) -> bool {
        matches!(self, Self::Streaming)
    }

    /// Check if the control channel should be encrypted
    #[must_use]
    pub fn requires_encryption(&self) -> bool {
        // After pairing completes, all control traffic is encrypted
        self.is_authenticated()
    }
}

/// State transition validation
impl Ap2SessionState {
    /// Attempt to transition to a new state
    ///
    /// # Errors
    ///
    /// Returns `StateError::InvalidTransition` if the transition is not allowed.
    #[allow(clippy::match_same_arms)]
    pub fn transition_to(&self, new_state: Ap2SessionState) -> Result<Ap2SessionState, StateError> {
        let valid = match (self, &new_state) {
            // From Connected
            (Self::Connected, Self::InfoExchanged) => true,
            (Self::Connected, Self::PairingSetup { step: 1 }) => true,

            // From InfoExchanged
            (Self::InfoExchanged, Self::PairingSetup { step: 1 }) => true,

            // Pairing setup progression
            (Self::PairingSetup { step: 1 }, Self::PairingSetup { step: 2 }) => true,
            (Self::PairingSetup { step: 2 }, Self::PairingSetup { step: 3 }) => true,
            (Self::PairingSetup { step: 3 }, Self::PairingSetup { step: 4 }) => true,
            (Self::PairingSetup { step: 4 }, Self::PairingVerify { step: 1 }) => true,

            // Pairing verify progression
            (Self::PairingVerify { step: 1 }, Self::PairingVerify { step: 2 }) => true,
            (Self::PairingVerify { step: 2 }, Self::PairingVerify { step: 3 }) => true,
            (Self::PairingVerify { step: 3 }, Self::PairingVerify { step: 4 }) => true,
            (Self::PairingVerify { step: 4 }, Self::Paired) => true,

            // From Paired
            (Self::Paired, Self::SetupPhase1) => true,

            // From SetupPhase1
            (Self::SetupPhase1, Self::SetupPhase2) => true,
            (Self::SetupPhase1, Self::Teardown) => true,

            // From SetupPhase2
            (Self::SetupPhase2, Self::Streaming) => true,
            (Self::SetupPhase2, Self::Teardown) => true,

            // From Streaming
            (Self::Streaming, Self::Paused) => true,
            (Self::Streaming, Self::Teardown) => true,

            // From Paused
            (Self::Paused, Self::Streaming) => true,
            (Self::Paused, Self::Teardown) => true,

            // Error can be reached from anywhere
            (_, Self::Error { .. }) => true,

            // Teardown can be reached from most states
            (_, Self::Teardown) if !matches!(self, Self::Connected | Self::Error { .. }) => true,

            _ => false,
        };

        if valid {
            Ok(new_state)
        } else {
            Err(StateError::InvalidTransition {
                from: format!("{self:?}"),
                to: format!("{new_state:?}"),
            })
        }
    }
}

/// Session state transition error
#[derive(Debug, thiserror::Error)]
pub enum StateError {
    /// Invalid state transition
    #[error("Invalid state transition from {from} to {to}")]
    InvalidTransition {
        /// Previous state
        from: String,
        /// Target state
        to: String,
    },
}
