"""Exercise the real serial adapter, unplug/reconnect, and restoration on Linux PTYs."""
import errno
import fcntl
import os
import pty
import select
import struct
import subprocess
import tempfile
import termios
import threading
import time
from pathlib import Path
from terminal_smoke import Screen, binary, drain


class Firmware:
    def __init__(self, link):
        self.master, self.slave = pty.openpty()
        self.name = os.ttyname(self.slave)
        link.unlink(missing_ok=True)
        link.symlink_to(self.name)
        self.commands = []
        self.error = None
        self.stop = threading.Event()
        self.thread = threading.Thread(target=self.run, daemon=True)
        self.thread.start()

    def run(self):
        pending = bytearray()
        queried = set()
        try:
            while not self.stop.is_set():
                if not select.select([self.master], [], [], 0.02)[0]:
                    continue
                pending.extend(os.read(self.master, 4096))
                while len(pending) >= 3:
                    assert pending[:2] == b"\x32\xac", pending
                    command = pending[2]
                    query = command == 0x20 or (command in (0, 4) and command not in queried)
                    length = 3 if query else {0: 4, 3: 4, 4: 4, 6: 42, 0x11: 4}.get(command)
                    if command == 0x10:
                        if len(pending) < 4:
                            break
                        length = 5 if pending[3] == 3 else 4
                    assert length is not None, pending
                    if len(pending) < length:
                        break
                    packet = bytes(pending[:length])
                    del pending[:length]
                    self.commands.append(packet)
                    # Restoring scrolling ends host ownership. The next feature
                    # queries brightness and scrolling again before taking over.
                    if command == 4 and not query and packet[3] == 1:
                        queried.clear()
                    if query:
                        queried.add(command)
                        response = bytearray(32)
                        if command == 0x20:
                            response[:3] = b"\x00\x20\x00"
                        else:
                            response[0] = 51 if command == 0 else 1
                        os.write(self.master, response[:3])
                        os.write(self.master, response[3:])
        except OSError as error:
            if not self.stop.is_set() and error.errno != errno.EIO:
                self.error = error
        except BaseException as error:
            self.error = error

    def close(self):
        self.stop.set()
        self.thread.join(timeout=1)
        os.close(self.master)
        os.close(self.slave)
        assert self.error is None, self.error


def main():
    with tempfile.TemporaryDirectory(prefix="inputmodule-serial-") as directory:
        link = Path(directory) / "matrix"
        firmware = Firmware(link)
        master, slave = pty.openpty()
        before = termios.tcgetattr(slave)
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
        proc = subprocess.Popen([binary, "--examples", "--serial-dev", str(link)], stdin=slave, stdout=slave, stderr=subprocess.PIPE)
        screen = Screen(80, 24)
        try:
            drain(master, screen, b"v0.2.0")
            os.write(master, b"\r")
            drain(master, screen, b"Snake | Score")
            drain(master, screen, timeout=0.25)
            assert any(packet[2] == 6 for packet in firmware.commands), firmware.commands
            assert b"\x32\xac\x04\x00" in firmware.commands
            # Unplug the serial endpoint while host gameplay is active.
            firmware.close()
            firmware = None
            drain(master, screen, b"Device:")
            # Reattach under the same explicit path and return to the menu.
            firmware = Firmware(link)
            os.write(master, b"q")
            drain(master, screen, b"v0.2.0", timeout=4)
            # Render live CPU/RAM bars through the real serial adapter after CPU warm-up.
            os.write(master, b"\x1b[B" * 4 + b"\r")
            drain(master, screen, b"CPU (left):")
            drain(master, screen, timeout=2.2)
            assert "warming up" not in screen.text(), screen.text()
            assert "RAM (right):" in screen.text(), screen.text()
            frames = [packet[3:] for packet in firmware.commands if packet[2] == 6]
            assert frames, firmware.commands
            # Four CPU columns, one dark separator, four RAM columns; 34 LEDs each.
            pixels = int.from_bytes(frames[-1], "little")
            assert all(pixels & (1 << (4 + 9 * row)) == 0 for row in range(34))
            assert pixels >> (9 * 34) == 0
            os.write(master, b"q")
            drain(master, screen, b"Framework")
            # The compact clock sends centered seven-segment frames to the device.
            clock_start = len(firmware.commands)
            os.write(master, b"\x1b[B" * 6 + b"\r")
            drain(master, screen, b"Current time:")
            drain(master, screen, timeout=0.2)
            clock_frames = [packet[3:] for packet in firmware.commands[clock_start:] if packet[2] == 6]
            assert clock_frames, firmware.commands[clock_start:]
            for frame in clock_frames:
                pixels = int.from_bytes(frame, "little")
                assert pixels != 0
                for row in range(34):
                    for col in range(9):
                        if col in (0, 8) or not 11 <= row < 22 or row == 16:
                            assert pixels & (1 << (col + 9 * row)) == 0
            os.write(master, b"q")
            drain(master, screen, b"Framework")
            # Start breathing, then exit through Ctrl-C and verify the original settings.
            os.write(master, b"\x1b[A\x1b[A\r")
            drain(master, screen, b"Brightness:")
            drain(master, screen, timeout=0.05)
            os.write(master, b"\x03")
            drain(master, screen, timeout=0.1)
            proc.wait(timeout=2)
            assert proc.returncode == 0, proc.stderr.read()
            deadline = time.monotonic() + 1
            while time.monotonic() < deadline and (not firmware.commands or firmware.commands[-1] != b"\x32\xac\x04\x01"):
                time.sleep(0.01)
            assert b"\x32\xac\x00\x33" in firmware.commands
            assert firmware.commands[-1] == b"\x32\xac\x04\x01", firmware.commands
            assert termios.tcgetattr(slave) == before
        finally:
            if proc.poll() is None:
                proc.kill()
                proc.wait()
            if firmware is not None:
                firmware.close()
            termios.tcsetattr(slave, termios.TCSANOW, before)
            proc.stderr.close()
            os.close(master)
            os.close(slave)
    print("Serial smoke checks passed")


if __name__ == "__main__":
    main()
