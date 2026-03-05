with open('src/connection/manager.rs', 'r') as f:
    content = f.read()

content = content.replace("""#[allow(
                                                    clippy::cast_possible_truncation,
                                                    clippy::cast_sign_loss
                                                )]""", """#[allow(
                                                    clippy::cast_possible_truncation,
                                                    clippy::cast_sign_loss,
                                                    reason = "Ports are u16, plist uses i64. Truncation is acceptable as ports fit in u16."
                                                )]""")

with open('src/connection/manager.rs', 'w') as f:
    f.write(content)
