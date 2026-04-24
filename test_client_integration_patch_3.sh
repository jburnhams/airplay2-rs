cat tests/client_integration.rs | sed 's/Ok(Err(e)) => { \/\* Connection failed as expected \*\//Ok(Err(_e)) => { \/\* Connection failed as expected *\//' > tests/client_integration_patched.rs
mv tests/client_integration_patched.rs tests/client_integration.rs
cargo test --test client_integration
