#!/usr/bin/env python3
import os
import sys

path = os.environ.get("SUMIKA_NOTIFY_RECORD")
if not path:
    sys.exit(0)
with open(path, "w", encoding="utf-8") as fh:
    fh.write("\t".join(sys.argv[1:]) + "\n")
env_path = os.environ.get("SUMIKA_NOTIFY_RECORD_ENV")
if env_path:
    with open(env_path, "w", encoding="utf-8") as fh:
        fh.write("session\t" + os.environ.get("SUMIKA_NOTIFY_SESSION", "") + "\n")
        fh.write("status\t" + os.environ.get("SUMIKA_NOTIFY_STATUS", "") + "\n")
