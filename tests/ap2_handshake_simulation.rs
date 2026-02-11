use airplay2::protocol::rtsp::{Headers, Method, RtspRequest};
use airplay2::receiver::ap2::request_handler::{
    Ap2HandleResult, Ap2Handlers, Ap2RequestContext, handle_ap2_request,
};
use airplay2::receiver::ap2::response_builder::Ap2ResponseBuilder;
use airplay2::receiver::ap2::session_state::Ap2SessionState;

fn make_request(method: Method, uri: &str) -> RtspRequest {
    let mut headers = Headers::new();
    headers.insert("CSeq".to_string(), "1".to_string());
    headers.insert("User-Agent".to_string(), "AirPlay/320.20".to_string());

    RtspRequest {
        method,
        uri: uri.to_string(),
        headers,
        body: vec![],
    }
}

trait ResponseExt {
    fn into_result(self) -> Ap2HandleResult;
}
impl ResponseExt for Ap2ResponseBuilder {
    fn into_result(self) -> Ap2HandleResult {
        Ap2HandleResult {
            response: self.encode(),
            new_state: None,
            event: None,
            error: None,
        }
    }
}

trait ResultExt {
    fn with_state(self, state: Ap2SessionState) -> Self;
}
impl ResultExt for Ap2HandleResult {
    fn with_state(mut self, state: Ap2SessionState) -> Self {
        self.new_state = Some(state);
        self
    }
}

#[test]
fn test_ap2_handshake_simulation() {
    let mut state = Ap2SessionState::Connected;

    // Mock handlers
    let handlers = Ap2Handlers {
        info: |_, cseq, _| {
            Ap2ResponseBuilder::ok()
                .cseq(cseq)
                .bplist_body(&airplay2::protocol::plist::PlistValue::Dictionary(
                    std::collections::HashMap::new(),
                ))
                .unwrap()
                .into_result()
                .with_state(Ap2SessionState::InfoExchanged)
        },
        pair_setup: |_, cseq, ctx| {
            // Return Step 1 (M1 received) or Step 3 (M3 received)
            // Test loop will simulate 1->2 (M2 sent) and 3->4 (M4 sent)
            let next_state = match ctx.state {
                Ap2SessionState::InfoExchanged | Ap2SessionState::Connected => {
                    Ap2SessionState::PairingSetup { step: 1 }
                }
                Ap2SessionState::PairingSetup { step: 2 } => {
                    Ap2SessionState::PairingSetup { step: 3 }
                }
                _ => ctx.state.clone(),
            };

            Ap2ResponseBuilder::ok()
                .cseq(cseq)
                .into_result()
                .with_state(next_state)
        },
        pair_verify: |_, cseq, ctx| {
            // Return Step 1 (M1 received) or Step 3 (M3 received)
            let next_state = match ctx.state {
                Ap2SessionState::PairingSetup { step: 4 }
                | Ap2SessionState::Connected
                | Ap2SessionState::InfoExchanged => {
                    Ap2SessionState::PairingVerify { step: 1 }
                }
                Ap2SessionState::PairingVerify { step: 2 } => {
                    Ap2SessionState::PairingVerify { step: 3 }
                }
                _ => ctx.state.clone(),
            };
            Ap2ResponseBuilder::ok()
                .cseq(cseq)
                .into_result()
                .with_state(next_state)
        },
        setup: |_, cseq, ctx| {
            let next_state = match ctx.state {
                Ap2SessionState::Paired => Ap2SessionState::SetupPhase1,
                Ap2SessionState::SetupPhase1 => Ap2SessionState::SetupPhase2,
                _ => ctx.state.clone(),
            };
            Ap2ResponseBuilder::ok()
                .cseq(cseq)
                .into_result()
                .with_state(next_state)
        },
        record: |_, cseq, ctx| {
            let next_state = match ctx.state {
                Ap2SessionState::SetupPhase2 | Ap2SessionState::Streaming => {
                    Ap2SessionState::Streaming
                }
                _ => ctx.state.clone(),
            };
            Ap2ResponseBuilder::ok()
                .cseq(cseq)
                .into_result()
                .with_state(next_state)
        },
        teardown: |_, cseq, _| {
            Ap2ResponseBuilder::ok()
                .cseq(cseq)
                .into_result()
                .with_state(Ap2SessionState::Teardown)
        },
        ..Ap2Handlers::default()
    };

    // 1. GET /info
    let req = make_request(Method::Get, "/info");
    let ctx = Ap2RequestContext {
        state: &state,
        session_id: None,
        encrypted: false,
        decrypt: None,
    };
    let res = handle_ap2_request(&req, &ctx, &handlers);
    if let Some(s) = res.new_state {
        state = state.transition_to(s).expect("Invalid info transition");
    }
    assert_eq!(state, Ap2SessionState::InfoExchanged);

    // 2. Pair Setup (2 requests)
    // Request 1 (M1)
    let req = make_request(Method::Post, "/pair-setup");
    let ctx = Ap2RequestContext {
        state: &state,
        session_id: None,
        encrypted: false,
        decrypt: None,
    };
    let res = handle_ap2_request(&req, &ctx, &handlers);
    if let Some(s) = res.new_state {
        state = state.transition_to(s).expect("Invalid setup M1 transition");
    }
    assert_eq!(state, Ap2SessionState::PairingSetup { step: 1 });

    // Simulate sending M2 (Step 1 -> Step 2)
    state = state
        .transition_to(Ap2SessionState::PairingSetup { step: 2 })
        .expect("Invalid setup 1->2 transition");

    // Request 2 (M3)
    let req = make_request(Method::Post, "/pair-setup");
    let ctx = Ap2RequestContext {
        state: &state,
        session_id: None,
        encrypted: false,
        decrypt: None,
    };
    let res = handle_ap2_request(&req, &ctx, &handlers);
    if let Some(s) = res.new_state {
        state = state.transition_to(s).expect("Invalid setup M3 transition");
    }
    assert_eq!(state, Ap2SessionState::PairingSetup { step: 3 });

    // Simulate sending M4 (Step 3 -> Step 4)
    state = state
        .transition_to(Ap2SessionState::PairingSetup { step: 4 })
        .expect("Invalid setup 3->4 transition");

    // 3. Pair Verify (2 requests)
    // Request 1 (M1)
    let req = make_request(Method::Post, "/pair-verify");
    let ctx = Ap2RequestContext {
        state: &state,
        session_id: None,
        encrypted: false,
        decrypt: None,
    };
    let res = handle_ap2_request(&req, &ctx, &handlers);
    if let Some(s) = res.new_state {
        state = state.transition_to(s).expect("Invalid verify M1 transition");
    }
    assert_eq!(state, Ap2SessionState::PairingVerify { step: 1 });

    // Simulate sending M2 (Step 1 -> Step 2)
    state = state
        .transition_to(Ap2SessionState::PairingVerify { step: 2 })
        .expect("Invalid verify 1->2 transition");

    // Request 2 (M3)
    let req = make_request(Method::Post, "/pair-verify");
    let ctx = Ap2RequestContext {
        state: &state,
        session_id: None,
        encrypted: false,
        decrypt: None,
    };
    let res = handle_ap2_request(&req, &ctx, &handlers);
    if let Some(s) = res.new_state {
        state = state.transition_to(s).expect("Invalid verify M3 transition");
    }
    assert_eq!(state, Ap2SessionState::PairingVerify { step: 3 });

    // Simulate sending M4 (Step 3 -> Step 4)
    state = state
        .transition_to(Ap2SessionState::PairingVerify { step: 4 })
        .expect("Invalid verify 3->4 transition");

    // Transition to Paired
    state = state
        .transition_to(Ap2SessionState::Paired)
        .expect("Invalid verify 4->Paired transition");

    assert_eq!(state, Ap2SessionState::Paired);

    // 4. SETUP (Phase 1)
    let req = make_request(Method::Setup, "rtsp://host/stream");
    let ctx = Ap2RequestContext {
        state: &state,
        session_id: Some("123"),
        encrypted: true,
        decrypt: None,
    };
    let res = handle_ap2_request(&req, &ctx, &handlers);
    if let Some(s) = res.new_state {
        state = state.transition_to(s).expect("Invalid Setup 1 transition");
    }
    assert_eq!(state, Ap2SessionState::SetupPhase1);

    // 5. SETUP (Phase 2)
    let req = make_request(Method::Setup, "rtsp://host/stream");
    let ctx = Ap2RequestContext {
        state: &state,
        session_id: Some("123"),
        encrypted: true,
        decrypt: None,
    };
    let res = handle_ap2_request(&req, &ctx, &handlers);
    if let Some(s) = res.new_state {
        state = state.transition_to(s).expect("Invalid Setup 2 transition");
    }
    assert_eq!(state, Ap2SessionState::SetupPhase2);

    // 6. RECORD
    let req = make_request(Method::Record, "rtsp://host/stream");
    let ctx = Ap2RequestContext {
        state: &state,
        session_id: Some("123"),
        encrypted: true,
        decrypt: None,
    };
    let res = handle_ap2_request(&req, &ctx, &handlers);
    if let Some(s) = res.new_state {
        state = state.transition_to(s).expect("Invalid Record transition");
    }
    assert_eq!(state, Ap2SessionState::Streaming);

    // 7. TEARDOWN
    let req = make_request(Method::Teardown, "rtsp://host/stream");
    let ctx = Ap2RequestContext {
        state: &state,
        session_id: Some("123"),
        encrypted: true,
        decrypt: None,
    };
    let res = handle_ap2_request(&req, &ctx, &handlers);
    if let Some(s) = res.new_state {
        state = state.transition_to(s).expect("Invalid Teardown transition");
    }
    assert_eq!(state, Ap2SessionState::Teardown);
}
