cat tests/client_integration.rs | sed 's/Ok(Err(_e)) => {/Ok(Err(e)) => { \/* Connection failed as expected *\//' > tests/client_integration_patched.rs
mv tests/client_integration_patched.rs tests/client_integration.rs
cargo test --test client_integration
