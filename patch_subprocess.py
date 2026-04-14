import re

with open('integration_tests/tests/common/subprocess.rs', 'r') as f:
    content = f.read()

content = content.replace(
    '#[allow(non_camel_case_types, dead_code)]',
    '#[allow(non_camel_case_types, dead_code, reason = "External API naming convention and shared test utilities")]'
)

with open('integration_tests/tests/common/subprocess.rs', 'w') as f:
    f.write(content)
