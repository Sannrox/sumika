#!/usr/bin/env python3
import fcntl
import signal
import struct
import sys
import termios
import time


def report(*_args):
    raw = fcntl.ioctl(sys.stdout.fileno(), termios.TIOCGWINSZ, b"\x00" * 8)
    rows, cols, _, _ = struct.unpack("HHHH", raw)
    sys.stdout.write(f"{rows}x{cols}\n")
    sys.stdout.flush()


signal.signal(signal.SIGWINCH, report)
report()
while True:
    time.sleep(0.2)
