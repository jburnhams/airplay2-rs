import os
import re

for root, dirs, files in os.walk('src'):
    for file in files:
        if file.endswith('.rs'):
            filepath = os.path.join(root, file)
            with open(filepath, 'r') as f:
                content = f.read()

            # Find all #[allow(...)]
            matches = re.finditer(r'#\[allow\(([^\]]+)\)\]', content)
            for m in matches:
                allow_content = m.group(1)
                if 'reason' not in allow_content:
                    print(f"{filepath}: {m.group(0)}")
