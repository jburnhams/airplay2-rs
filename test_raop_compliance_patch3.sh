cat tests/raop_compliance.rs | sed 's/Ok(Err(_)) => println!("Client panic"),/Ok(Err(e)) => std::panic::resume_unwind(e.into_panic()),/' > tests/raop_compliance_patched.rs
mv tests/raop_compliance_patched.rs tests/raop_compliance.rs
cargo test --test raop_compliance
