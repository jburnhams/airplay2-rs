cat tests/raop_compliance.rs | sed 's/Ok(Err(_)) => println!("Client panic"),/Ok(Err(e)) => std::panic::resume_unwind(e.into_panic()),/' > tests/raop_compliance_patched.rs
mv tests/raop_compliance_patched.rs tests/raop_compliance.rs

cat tests/raop_compliance.rs | sed 's/Err(_) => println!("Timeout waiting for client"),/Err(_) => { \/* Timeout is acceptable for partial handshake test *\/ }/' > tests/raop_compliance_patched.rs
mv tests/raop_compliance_patched.rs tests/raop_compliance.rs

cat tests/receiver/protocol_tests.rs | sed 's/Err(_) => return false,/Err(_) => std::panic!("Timeout waiting for volume event"),/' > tests/receiver/protocol_tests_patched.rs
mv tests/receiver/protocol_tests_patched.rs tests/receiver/protocol_tests.rs

cat tests/client_integration.rs | sed 's/Err(_) => {/Err(_) => { \/* Timeout is also an acceptable failure mode depending on OS *\//' > tests/client_integration_patched.rs
mv tests/client_integration_patched.rs tests/client_integration.rs

cargo test --test raop_compliance
cargo test --test client_integration
cargo test --test receiver
