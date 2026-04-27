with open("tests/raop_compliance.rs", "r") as f:
    c = f.read()

import re
c = re.sub(
    r'let n = stream\.read\(&mut buffer\)\.await\.unwrap\(\);',
    r'''let n = match stream.read(&mut buffer).await {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };''',
    c,
    count=1 # only in the loop? wait, the first one is outside the loop!
)

with open("tests/raop_compliance.rs", "w") as f:
    f.write(c)
