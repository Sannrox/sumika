#!/usr/bin/env python3
import json
import sys
import time

path = sys.argv[1]
with open(path, "w", encoding="utf-8") as fh:
    json.dump(sys.argv, fh)
    fh.flush()
time.sleep(60)
