#!/usr/bin/env python3
"""Linux PTY smoke tests; first build the debug respiro-cli binary."""
import fcntl
import os
from pathlib import Path
import pty
import re
import select
import signal
import struct
import subprocess
import tempfile
import termios
import time

BINARY = Path(__file__).resolve().parents[1] / "target/debug/respiro-cli"
ANSI_SGR = re.compile(rb"\x1b\[[0-9;]*m")


class Session:
    def __init__(self, *args, expected_start=b"Inhale", extra_env=None):
        self.master, self.slave = pty.openpty()
        self.resize(80, 24)
        self.original = termios.tcgetattr(self.slave)
        env = {**os.environ, "TERM": "xterm"}
        env.pop("RESPIRO_LANG", None)
        env.pop("NO_COLOR", None)
        env.pop("RESPIRO_CONFIG", None)
        if extra_env:
            env.update(extra_env)
        self.process = subprocess.Popen(
            [str(BINARY), *args], stdin=self.slave, stdout=self.slave,
            stderr=self.slave, env=env,
        )
        self.output = b""
        deadline = time.monotonic() + 3
        while expected_start not in self.output and time.monotonic() < deadline:
            self.read(0.05)
        assert expected_start in self.output, self.output

    def resize(self, width, height):
        fcntl.ioctl(self.slave, termios.TIOCSWINSZ, struct.pack("HHHH", height, width, 0, 0))

    def read(self, duration):
        deadline = time.monotonic() + duration
        chunk = b""
        while time.monotonic() < deadline:
            if select.select([self.master], [], [], max(0, deadline - time.monotonic()))[0]:
                chunk += os.read(self.master, 65536)
        self.output += chunk
        return chunk

    def finish(self):
        deadline = time.monotonic() + 4
        while self.process.poll() is None and time.monotonic() < deadline:
            self.read(0.05)
        assert self.process.poll() == 0, self.output[-1000:]
        self.read(0.05)
        assert termios.tcgetattr(self.slave) == self.original, "terminal not restored"
        assert b"\x1b[?25h" in self.output, "cursor not restored"
        if b"\x1b[?1049h" in self.output:
            assert b"\x1b[?1049l" in self.output, "alternate screen not restored"

    def close(self):
        if self.process.poll() is None:
            self.process.kill()
        self.process.wait()
        os.close(self.master)
        os.close(self.slave)


def check_exit(key=None, sig=None, compact=False):
    session = Session(*(["--compact"] if compact else []))
    try:
        if sig:
            session.process.send_signal(sig)
        else:
            os.write(session.master, key)
        session.finish()
    finally:
        session.close()


for key in [b"q", b"\x1b", b"\x03"]:
    check_exit(key=key)
for sig in [signal.SIGINT, signal.SIGTERM, signal.SIGHUP]:
    check_exit(sig=sig)
check_exit(key=b"q", compact=True)

session = Session("--pattern", "1-1", "--cycles", "1", "--animation", "path", "--bell")
try:
    assert b"\x1b[1;36m" in session.output, "expected ANSI phase color by default"
    os.write(session.master, b" ")
    session.read(0.15)
    frozen = ANSI_SGR.sub(b"", session.read(2.1))
    assert session.process.poll() is None, "pause consumed the cycle limit"
    assert b"Paused" in frozen
    bars = re.findall(rb"\[[=.o]+\]", frozen)
    assert len(bars) > 1 and len(set(bars)) == 1, "paused progress moved"
    session.resize(12, 4)
    session.read(0.1)
    session.resize(100, 30)
    os.write(session.master, b"r ")
    session.finish()
    assert b"Exhale" in session.output
    assert b"\x07" in session.output, "expected terminal bell on phase transition"
finally:
    session.close()

with tempfile.NamedTemporaryFile("w", suffix=".conf", delete=False) as cfg:
    cfg.write("auto_hide = true\nauto_hide_secs = 0.5\n")
    cfg_path = cfg.name

try:
    session_es = Session(
        "--lang", "es", "--no-color", "--pattern", "1-1",
        expected_start=b"Inhala", extra_env={"RESPIRO_CONFIG": cfg_path},
    )
    try:
        assert b"\x1b[1;36m" not in session_es.output, "--no-color did not disable colors"
        os.write(session_es.master, b"a")
        chunk = session_es.read(0.25)
        assert b"@" in chunk, "live animation toggle 'a' did not switch to path"
        session_es.read(0.5)
        later = session_es.read(0.3)
        assert b"espacio pausa" not in later, "auto_hide from config file did not hide controls"
        os.write(session_es.master, b" ")
        session_es.read(0.2)
        assert b"Pausa" in session_es.output
        os.write(session_es.master, b"q")
        session_es.finish()
    finally:
        session_es.close()
finally:
    os.unlink(cfg_path)

for animation in ["orb", "path"]:
    result = subprocess.run([str(BINARY), "--snapshot", "--animation", animation], capture_output=True, check=True)
    assert result.stdout.isascii() and b"\x1b" not in result.stdout
    assert not re.search(rb"\d", result.stdout)
result = subprocess.run([str(BINARY)], capture_output=True)
assert result.returncode != 0 and b"--snapshot" in result.stderr
print("PTY OK: keyboard, signals, pause, reset, resize, live keys, bell, colors, config auto-hide, lang (en/es), cleanup and snapshots")
