cat tests/client_integration.rs | sed 's/Err(_) => {/Err(_) => { \/* Timeout is also an acceptable failure mode depending on OS *\//' > tests/client_integration_patched.rs
mv tests/client_integration_patched.rs tests/client_integration.rs
cargo test --test client_integration
