with open('src/protocol/mod.rs', 'r') as f:
    content = f.read()

content = content.replace(
    '#![allow(missing_docs)]',
    '#![allow(missing_docs, reason = "Protocol implementations are internal and mostly self-explanatory")]'
)

with open('src/protocol/mod.rs', 'w') as f:
    f.write(content)
