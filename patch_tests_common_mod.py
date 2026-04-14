with open('tests/common/mod.rs', 'r') as f:
    content = f.read()

content = content.replace(
    '#![allow(dead_code)]',
    '#![allow(dead_code, reason = "Shared test utility functions may not be fully utilized by every test file")]'
)

with open('tests/common/mod.rs', 'w') as f:
    f.write(content)
