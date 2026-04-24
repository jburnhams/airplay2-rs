cat tests/receiver/protocol_tests.rs | sed 's/Err(_) => return false,/Err(_) => std::panic!("Timeout waiting for volume event"),/' > tests/receiver/protocol_tests_patched.rs
mv tests/receiver/protocol_tests_patched.rs tests/receiver/protocol_tests.rs
cargo test --test receiver
