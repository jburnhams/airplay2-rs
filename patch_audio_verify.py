import re

with open('integration_tests/tests/common/audio_verify.rs', 'r') as f:
    content = f.read()

# Add `#![allow(dead_code, reason = "Shared test utility functions")]` at the top
content = '#![allow(dead_code, reason = "Shared test utility functions")]\n\n' + content

# Remove all `#[allow(dead_code)]` lines (allowing spaces/tabs before)
content = re.sub(r'[ \t]*#\[allow\(dead_code\)\]\n', '', content)

with open('integration_tests/tests/common/audio_verify.rs', 'w') as f:
    f.write(content)
