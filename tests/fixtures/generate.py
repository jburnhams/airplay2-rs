#!/usr/bin/env python3
"""Regenerate the binary plist fixtures with Python's plistlib.

The fixtures are written by an independent encoder on purpose, so the Rust
decoder is tested against bytes it did not produce itself. Run from the repo
root: python3 tests/fixtures/generate.py
"""
import datetime
import math
import pathlib
import plistlib

OUT = pathlib.Path(__file__).parent

FIXTURES = {
    "simple_dict.bplist": {"key": "value", "int": 42, "bool": True},
    "nested_dict.bplist": {"parent": {"child": "hello", "grandchild": 123}},
    "array.bplist": [1, "two", False],
    "large_dict.bplist": {f"key_{i}": i for i in range(100)},
    "types.bplist": {
        "data": b"\xca\xfe\xba\xbe",
        "date": datetime.datetime(2001, 1, 1),
        "real": math.pi,
    },
}

for name, value in FIXTURES.items():
    (OUT / name).write_bytes(plistlib.dumps(value, fmt=plistlib.FMT_BINARY, sort_keys=True))
    print(f"wrote {name}")
