cat tests/raop_compliance.rs | sed 's/Err(_) => println!("Timeout waiting for client"),/Err(_) => std::panic!("Timeout waiting for client"),/' > tests/raop_compliance_patched.rs
mv tests/raop_compliance_patched.rs tests/raop_compliance.rs
cargo test --test raop_compliance
