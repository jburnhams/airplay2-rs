import re

with open('src/protocol/rtsp/mod.rs', 'r') as f:
    content = f.read()

pattern = r'#\!\[allow\(unused_imports\)\]\n#\!\[allow\(dead_code\)\]\n'
replacement = '#![allow(unused_imports, dead_code, reason = "Legacy module with some unused features")]\n'

content = re.sub(pattern, replacement, content)

with open('src/protocol/rtsp/mod.rs', 'w') as f:
    f.write(content)
