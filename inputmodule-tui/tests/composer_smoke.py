"""Widget editor save/load, headless playback, and serial reconnect without a TUI."""
import json
import signal
import subprocess
import tempfile
import time
from pathlib import Path
from terminal_smoke import binary, session
from serial_smoke import Firmware


def wait_for_frames(firmware, count=1, timeout=3):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        frames = [packet[3:] for packet in firmware.commands if packet[2] == 6]
        if len(frames) >= count:
            return frames
        assert firmware.error is None, firmware.error
        time.sleep(0.02)
    raise AssertionError(f"Missing frames: {firmware.commands!r}, error={firmware.error!r}")


with tempfile.TemporaryDirectory(prefix="inputmodule-composer-") as directory:
    root = Path(directory)
    saved = root / "my layout.json"
    session([
        (b"\x1b[B" * 6, b"> Clock (7-segment)"),
        (b"\r", b"Move with arrows"),
        (b"\x1b[C", b"Position: (1, 0)"),
        (b"\x1b[B" * 2, b"Position: (1, 2)"),
        (b"\r", b"Widget placed"),
        (b"\t\x1b[A\x1b[A", b"System Monitor requires 9x12"),
        (b"\r", b"Move with arrows"),
        (b"\r", b"Widget placed"),
        (b"\t" + b"\x1b[B" * 3, b"Equalizer requires 9x10"),
        (b"\r", b"No free rectangle fits this widget"),
        # Click the disabled Snake row; it must not replace or add anything.
        (b"\x1b[<0;28;3M\x1b[<0;28;3m", None),
        (b"s", b"Saved "),
        (b"q", None),
    ], args=("--save-template", str(saved)), startup=b"Widget Composer")
    template = json.loads(saved.read_text())
    assert template == {
        "version": 1, "canvas": {"width": 9, "height": 34}, "widgets": [
            {"kind": "seven_segment_clock", "x": 1, "y": 2},
            {"kind": "system_monitor", "x": 0, "y": 13},
        ],
    }, template
    # Load into the editor and save again, preserving the exact template.
    session([(b"s", b"Saved "), (b"q", None)], args=("--template", str(saved)), startup=b"Widget Composer")
    assert json.loads(saved.read_text()) == template
    session([(b"s", b"Save failed:"), (b"q", None)],
            args=("--save-template", str(root / "missing" / "layout.json")), startup=b"Widget Composer")
    session([(b"\x03", None)], width=63, height=23, args=(), startup=b"Terminal too small")

    validation = subprocess.run([binary, "--template", str(saved), "--validate-template"], capture_output=True, timeout=2)
    assert validation.returncode == 0 and b"2 widgets" in validation.stdout, validation.stderr
    playback = subprocess.run([binary, "--template", str(saved), "--headless", "--no-device", "--duration", "1"], capture_output=True, timeout=3)
    assert playback.returncode == 0 and b"\x1b[" not in playback.stdout + playback.stderr, playback.stderr
    invalid = root / "invalid.json"
    invalid.write_text(json.dumps({**template, "widgets": [template["widgets"][0], template["widgets"][0]]}))
    rejected = subprocess.run([binary, "--template", str(invalid), "--headless", "--serial-dev", "never-open-this-port"], capture_output=True, timeout=2)
    assert rejected.returncode != 0 and b"overlaps" in rejected.stderr, rejected.stderr
    assert b"\x1b[" not in rejected.stdout + rejected.stderr

    # Life changes frames deterministically, allowing unplug detection without waiting for keepalive.
    template["widgets"][1] = {"kind": "game_of_life", "x": 0, "y": 13, "pattern": "glider"}
    live = root / "live.json"
    live.write_text(json.dumps(template))
    link = root / "matrix"
    firmware = Firmware(link)
    proc = subprocess.Popen([binary, "--headless", "--template", str(live), "--serial-dev", str(link)],
                            stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    try:
        frames = wait_for_frames(firmware, count=2)
        for frame in frames:
            pixels = int.from_bytes(frame, "little")
            for row in range(34):
                for col in range(9):
                    within = (1 <= col < 8 and 2 <= row < 13) or 13 <= row < 25
                    assert within or pixels & (1 << (col + 9 * row)) == 0
        assert proc.poll() is None
        firmware.close()
        firmware = None
        time.sleep(0.4)
        firmware = Firmware(link)
        wait_for_frames(firmware, count=2, timeout=5)
        proc.send_signal(signal.SIGTERM)
        stdout, stderr = proc.communicate(timeout=2)
        assert proc.returncode == 0, stderr
        assert b"\x1b[" not in stdout + stderr
        deadline = time.monotonic() + 1
        while time.monotonic() < deadline and firmware.commands[-1] != b"\x32\xac\x04\x01":
            time.sleep(0.01)
        assert b"\x32\xac\x00\x33" in firmware.commands
        assert firmware.commands[-1] == b"\x32\xac\x04\x01", firmware.commands
    finally:
        if proc.poll() is None:
            proc.kill()
            proc.wait()
        if firmware is not None:
            firmware.close()
        proc.stdout.close()
        proc.stderr.close()

print("Composer and headless smoke checks passed")
