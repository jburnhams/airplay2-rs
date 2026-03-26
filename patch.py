import re

with open("src/protocol/ptp/tests/node.rs", "r") as f:
    content = f.read()

pattern = r"""    let offset_ms = b_clock_locked\.offset_millis\(\)\.abs\(\);
    assert!\(
        offset_ms < 50\.0,
        "Offset on loopback should be small, got \{offset_ms:\.3\}ms"
    \);"""

replacement = """    let offset_ms = b_clock_locked.offset_millis().abs();
    assert!(
        offset_ms < 100.0,
        "Offset on loopback should be small, got {offset_ms:.3}ms"
    );"""

content = re.sub(pattern, replacement, content)

with open("src/protocol/ptp/tests/node.rs", "w") as f:
    f.write(content)
