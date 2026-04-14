with open('src/protocol/ptp/tests/node.rs', 'r') as f:
    content = f.read()

content = content.replace(
    '#[allow(clippy::no_effect_underscore_binding)]',
    '#[allow(clippy::no_effect_underscore_binding, reason = "Kept for documentation purposes to show T1 reference")]'
)

with open('src/protocol/ptp/tests/node.rs', 'w') as f:
    f.write(content)
