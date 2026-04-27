with open("tests/raop_compliance.rs", "r") as f:
    c = f.read()
import re
c = re.sub(r'\\\\r\\\\n', r'\\r\\n', c)
with open("tests/raop_compliance.rs", "w") as f:
    f.write(c)
