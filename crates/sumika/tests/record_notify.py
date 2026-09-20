#!/usr/bin/env python3
import os
import sys

path = os.environ.get("SUMIKA_NOTIFY_RECORD")
if not path:
    sys.exit(0)
with open(path, "w", encoding="utf-8") as fh:
    fh.write("\t".join(sys.argv[1:]) + "\n")
