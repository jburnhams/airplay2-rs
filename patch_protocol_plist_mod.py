import re

with open('src/protocol/plist/mod.rs', 'r') as f:
    content = f.read()

pattern = r'#\!\[allow\(dead_code\)\]\n#\!\[allow\(unused_imports\)\]\n#\!\[allow\(missing_docs\)\]\n#\!\[allow\(\n    clippy::all,\n    clippy::pedantic,\n    clippy::nursery,\n    reason = "Legacy module"\n\)\]\n'
replacement = '#![allow(\n    dead_code,\n    unused_imports,\n    missing_docs,\n    clippy::all,\n    clippy::pedantic,\n    clippy::nursery,\n    reason = "Legacy module"\n)]\n'

content = re.sub(pattern, replacement, content)

with open('src/protocol/plist/mod.rs', 'w') as f:
    f.write(content)
