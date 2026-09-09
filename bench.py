"""Self-contained benchmark entry point; no qns or Python binding required."""
from __future__ import annotations
import subprocess
import sys
from pathlib import Path

if __name__ == "__main__":
    raise SystemExit(subprocess.call(
        ["cargo", "run", "--release", "-p", "z180-cli", "--", "bench", *sys.argv[1:]],
        cwd=Path(__file__).resolve().parent,
    ))
