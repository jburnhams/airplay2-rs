import re

with open('src/audio/mod.rs', 'r') as f:
    content = f.read()

pattern = r'#\!\[allow\(unused_imports\)\]\n#\!\[allow\(dead_code\)\]\n'
replacement = '#![allow(unused_imports, dead_code, reason = "Pending implementation features")]\n'

content = re.sub(pattern, replacement, content)

with open('src/audio/mod.rs', 'w') as f:
    f.write(content)
