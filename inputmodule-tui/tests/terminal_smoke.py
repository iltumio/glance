"""Linux terminal regression checks; no third-party dependencies or device needed."""
import codecs
import re
import errno
import fcntl
import os
import pty
import select
import struct
import subprocess
import sys
import termios
import time
from pathlib import Path

binary = str(Path(sys.argv[1]).resolve())


class Screen:
    """Reconstruct text from ratatui's incremental VT cursor updates."""
    def __init__(self, width, height):
        self.width, self.height = width, height
        self.cells = [[" "] * width for _ in range(height)]
        self.row = self.col = 0
        self.decoder = codecs.getincrementaldecoder("utf-8")()
        self.pending = ""

    def feed(self, data):
        self.pending += self.decoder.decode(data)
        i = 0
        while i < len(self.pending):
            char = self.pending[i]
            if char == "\x1b":
                match = re.match(r"\x1b\[([0-9;?]*)([A-Za-z])", self.pending[i:])
                if not match:
                    break
                params, command = match.groups()
                values = [int(n or 0) for n in params.lstrip("?").split(";")]
                if command in "Hf":
                    self.row = max(0, (values[0] or 1) - 1)
                    self.col = max(0, ((values[1] if len(values) > 1 else 1) or 1) - 1)
                elif command == "J" and values[0] == 2:
                    self.cells = [[" "] * self.width for _ in range(self.height)]
                elif command == "K" and self.row < self.height:
                    for column in range(self.col, self.width):
                        self.cells[self.row][column] = " "
                i += match.end()
                continue
            if char == "\r":
                self.col = 0
            elif char == "\n":
                self.row += 1
            elif ord(char) >= 32:
                if self.row < self.height and self.col < self.width:
                    self.cells[self.row][self.col] = char
                self.col += 1
            i += 1
        self.pending = self.pending[i:]

    def text(self):
        return "\n".join("".join(row) for row in self.cells)


def drain(master, output, until=None, timeout=2):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if until is not None and until.decode() in output.text():
            return
        if select.select([master], [], [], 0.02)[0]:
            try:
                output.feed(os.read(master, 65536))
            except OSError as error:
                if error.errno == errno.EIO:
                    return
                raise
    if until is not None:
        raise AssertionError(f"Missing {until!r}: {output.text()!r}")


def session(keys, width=80, height=24, expected=None, args=("--examples",), startup=b"Framework"):
    master, slave = pty.openpty()
    before = termios.tcgetattr(slave)
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", height, width, 0, 0))
    proc = subprocess.Popen([binary, "--no-device", *args], stdin=slave, stdout=slave, stderr=subprocess.PIPE)
    output = Screen(width,height)
    try:
        drain(master, output, startup)
        for keys_to_send, text_to_wait in keys:
            os.write(master, keys_to_send)
            if text_to_wait:
                drain(master, output, text_to_wait)
            else:
                drain(master, output, timeout=0.1)
        drain(master, output, timeout=0.1)
        proc.wait(timeout=2)
        assert proc.returncode == 0, proc.stderr.read()
        after = termios.tcgetattr(slave)
        assert after == before, "Terminal attributes were not restored"
        if expected:
            assert expected.decode() in output.text(), output.text()
    finally:
        if proc.poll() is None:
            proc.kill()
            proc.wait()
        termios.tcsetattr(slave, termios.TCSANOW, before)
        proc.stderr.close()
        os.close(master)
        os.close(slave)


if __name__ == "__main__":
    help_result = subprocess.run([binary, "--help"], capture_output=True, timeout=2)
    assert help_result.returncode == 0 and b"--no-device" in help_result.stdout
    invalid = subprocess.run([binary, "--no-device", "--serial-dev", "COM3"], capture_output=True, timeout=2)
    assert invalid.returncode != 0
    session([(b"q", None)])
    session([(b"\r", b"Snake | Score"), (b"\x03", None)])
    session([(b"\r", b"Terminal too small"), (b"\x03", None)], width=19, height=22)
    # The last menu item is WPM. Ctrl-C must remain a quit command while typing.
    session([(b"\x1b[A", b"> WPM Demo"), (b"\r", b"Type to measure"), (b"hello", None), (b"\x03", None)])
    # The monitor uses real host metrics and remains interactive during warm-up.
    session([(b"\x1b[B" * 4, b"> System Monitor"), (b"\r", b"CPU (left):"), (b"q", b"Framework"), (b"q", None)])
    session([(b"\x1b[B" * 6, b"> Clock (7-segment)"), (b"\r", b"Current time:"), (b"\x03", None)])
    # An I/O failure after enabling raw mode must clean up partial initialization.
    master, slave = pty.openpty()
    before = termios.tcgetattr(slave)
    try:
        with open("/dev/full", "wb", buffering=0) as failed_output:
            result = subprocess.run([binary, "--no-device"], stdin=slave, stdout=failed_output, stderr=subprocess.PIPE, timeout=2)
        assert result.returncode != 0
        assert termios.tcgetattr(slave) == before, "Partial initialization leaked raw mode"
    finally:
        termios.tcsetattr(slave, termios.TCSANOW, before)
        os.close(master)
        os.close(slave)
    print("Terminal smoke checks passed")
