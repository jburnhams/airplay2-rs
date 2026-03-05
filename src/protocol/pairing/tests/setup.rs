use crate::protocol::pairing::PairingError;
use crate::protocol::pairing::setup::PairSetup;
use crate::protocol::pairing::tlv::{TlvEncoder, TlvType, errors};

#[test]
fn test_pair_setup_failures() {
    // Test that PairSetup correctly handles device error codes
    let mut setup = PairSetup::new();
    setup.set_pin("1234");
    let _ = setup.start().unwrap();

    // Simulate M2 error from device
    let m2 = TlvEncoder::new()
        .add_state(2)
        .add_byte(TlvType::Error, errors::BUSY)
        .build();

    let result = setup.process_m2(&m2);
    assert!(matches!(result, Err(PairingError::DeviceError { code: 7 }))); // BUSY is 0x07
}
