with open("tests/raop_compliance.rs", "r") as f:
    c = f.read()

import re

c = re.sub(
    r'''let n = match stream\.read\(&mut buffer\)\.await \{
\s*Ok\(0\) \| Err\(\_\) => break,
\s*Ok\(n\) => n,
\s*\};''',
    r'''let n = match stream.read(&mut buffer).await {
        Ok(0) | Err(_) => return,
        Ok(n) => n,
    };''',
    c,
    count=1
)

with open("tests/raop_compliance.rs", "w") as f:
    f.write(c)
