#!/usr/bin/env python3
"""Punto de entrada estándar del proyecto: dev, build y release.

Uso:
  tools/build.py dev              # cargo run -p notty (debug)
  tools/build.py build            # cargo build --release (workspace)
  tools/build.py release <tag> [notes_file]   # delega en tools/release.ps1
"""

import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def dev() -> int:
    return subprocess.call(["cargo", "run", "-p", "notty"], cwd=ROOT)


def build() -> int:
    return subprocess.call(["cargo", "build", "--release"], cwd=ROOT)


def release(args: list[str]) -> int:
    if not args:
        print("Uso: tools/build.py release <tag> [notes_file]", file=sys.stderr)
        return 2
    tag = args[0]
    notes_file = args[1] if len(args) > 1 else "CHANGELOG.md"
    cmd = [
        "pwsh", "-File", str(ROOT / "tools" / "release.ps1"),
        "-Tag", tag, "-NotesFile", notes_file,
    ]
    return subprocess.call(cmd, cwd=ROOT)


def main() -> int:
    if len(sys.argv) < 2:
        print(__doc__)
        return 2
    cmd, rest = sys.argv[1], sys.argv[2:]
    if cmd == "dev":
        return dev()
    if cmd == "build":
        return build()
    if cmd == "release":
        return release(rest)
    print(f"Comando desconocido: {cmd}", file=sys.stderr)
    print(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main())
