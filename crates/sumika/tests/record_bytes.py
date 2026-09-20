#!/usr/bin/env python3
import sys

path = sys.argv[1]
with open(path, "ab", buffering=0) as out:
    while True:
        chunk = sys.stdin.buffer.read(1)
        if not chunk:
            break
        out.write(chunk)
        out.flush()
