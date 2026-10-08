"""File polling and JSON-RPC updates through the real CLI, HTTP server and serial wire."""
import json
import os
import re
import select
import signal
import socket
import subprocess
import tempfile
import time
import urllib.error
import urllib.request
from pathlib import Path
from terminal_smoke import binary, session
from serial_smoke import Firmware


def encoded_pixels(cells):
    return sum(1 << (x + 9 * y) for x, y in cells)


def wait_frame(firmware, expected, timeout=3):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        frames = [int.from_bytes(p[3:], "little") for p in firmware.commands if p[2] == 6]
        if expected in frames:
            return
        assert firmware.error is None, firmware.error
        time.sleep(0.02)
    raise AssertionError(f"Missing frame {expected:x}; frames={frames!r}")


with tempfile.TemporaryDirectory(prefix="inputmodule-inputs-") as directory:
    root = Path(directory)
    data = root / "value.json"
    data.write_text('{"percent":50}')
    saved = root / "editor.json"
    session([
        (b"\x1b[A", b"> Progress Bar 3x1"),
        (b"\r", b"Move with arrows"), (b"\r", b"Widget placed"),
        (b"e", b"Configure Progress Bar"),
        (b"\x1b[3~filebar\x1b[B\x1b[3~vertical\x1b[B\x1b[3~10\x1b[B\x1b[3~2", None),
        (b"\x1b[B" * 5 + b"\x1b[3~file\x1b[B\x1b[3~value.json\x1b[B\x1b[3~100\x1b[B\x1b[3~json_pointer\x1b[B\x1b[3~/percent", None),
        (b"\r", b"Widget configured"),
        (b"", b"Progress: 50.0%"),
        (b"s", b"Saved "), (b"q", None),
    ], args=("--save-template", str(saved)), startup=b"Widget Composer")
    widget = json.loads(saved.read_text())["widgets"][0]
    assert widget["name"] == "filebar" and widget["progress"]["length"] == 10
    assert widget["progress"]["width"] == 2 and widget["progress"]["orientation"] == "vertical"
    assert widget["input"] == {"type":"file", "path":"value.json", "interval_ms":100, "mask":{"type":"json_pointer", "pointer":"/percent"}}, widget

    text = root / "progress.txt"
    text.write_text("progress=25%\n")
    layout = root / "live.json"
    widgets = [
        {"kind":"progress_bar", "name":"file", "x":0,"y":2,
         "progress":{"length":8,"width":1},
         "input":{"type":"file","path":"progress.txt","interval_ms":100,"mask":{"type":"regex","pattern":r"progress=(\d+)%","group":1}}},
        {"kind":"progress_bar", "name":"api-one", "x":0,"y":5,
         "progress":{"orientation":"vertical","length":10,"width":2}, "input":{"type":"api"}},
        {"kind":"progress_bar", "name":"api-two", "x":0,"y":17,
         "progress":{"length":9,"width":1}, "input":{"type":"api"}},
    ]
    layout.write_text(json.dumps({"version":1,"canvas":{"width":9,"height":34},"widgets":widgets}))
    validation = subprocess.run([binary,"--template",str(layout),"--validate-template"],capture_output=True,timeout=2)
    assert validation.returncode == 0, validation.stderr
    link = root / "matrix"
    firmware = Firmware(link)
    proc = subprocess.Popen([binary,"--template",str(layout),"--headless","--serial-dev",str(link),"--api-listen","127.0.0.1:0"], stdin=subprocess.DEVNULL,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
    try:
        stderr = bytearray()
        deadline = time.monotonic() + 3
        match = None
        while time.monotonic() < deadline:
            if select.select([proc.stderr], [], [], 0.05)[0]:
                stderr.extend(os.read(proc.stderr.fileno(),65536))
            match = re.search(rb"API: http://127.0.0.1:(\d+)/rpc", stderr)
            if match:
                break
            assert proc.poll() is None, stderr
        assert match, stderr
        port = int(match[1]); url = f"http://127.0.0.1:{port}/rpc"
        def post(payload, raw=False):
            body = payload if raw else json.dumps(payload).encode()
            request = urllib.request.Request(url,body,{"Content-Type":"application/json"})
            with urllib.request.urlopen(request,timeout=2) as response:
                data = response.read()
                return response.status, json.loads(data) if data else None
        def update(name, value, ident=1):
            return {"jsonrpc":"2.0","id":ident,"method":"widget.update","params":{"widget":name,"values":{"value":value}}}
        wait_frame(firmware, encoded_pixels([(0,2),(1,2)]))
        status, result = post(update("api-one",50))
        assert result["result"]["values"]["value"] == 50 and result["id"] == 1
        vertical = [(x,y) for x in range(2) for y in range(10,15)]
        wait_frame(firmware, encoded_pixels(vertical + [(0,2),(1,2)]))
        batch = [update("api-two",100,2), {"jsonrpc":"2.0","id":3,"method":"widget.get","params":{"widget":"api-one"}}]
        _, results = post(batch)
        assert results[1]["result"]["values"]["value"] == 50
        horizontal = [(x,17) for x in range(9)]
        text.write_text("progress=75%\n")
        wait_frame(firmware, encoded_pixels(vertical + horizontal + [(x,2) for x in range(6)]))
        assert post(update("missing",10))[1]["error"]["code"] == -32001
        assert post(update("file",10))[1]["error"]["code"] == -32602
        assert post(update("api-one","bad"))[1]["error"]["code"] == -32602
        assert post(b"{broken",raw=True)[1]["error"]["code"] == -32700
        assert post([])[1]["error"]["code"] == -32600
        assert post({"jsonrpc":"2.0","method":"widget.update","params":{"widget":"api-two","values":{"value":100}}}) == (204,None)
        assert post([{"jsonrpc":"2.0","method":"widget.list"}]) == (204,None)
        try:
            post(b" " * 65_537,raw=True)
            raise AssertionError("Oversized API body accepted")
        except urllib.error.HTTPError as error:
            assert error.code == 413
        text.write_text("invalid")
        time.sleep(0.2)
        text.write_text("progress=50%\n")
        wait_frame(firmware, encoded_pixels(vertical + horizontal + [(x,2) for x in range(4)]))
        # Inputs remain live through serial disconnect/reconnect.
        firmware.close(); firmware = None
        time.sleep(0.2)
        post(update("api-one",80))
        firmware = Firmware(link)
        vertical = [(x,y) for x in range(2) for y in range(7,15)]
        wait_frame(firmware, encoded_pixels(vertical + horizontal + [(x,2) for x in range(4)]),timeout=5)
        proc.send_signal(signal.SIGTERM)
        stdout, tail = proc.communicate(timeout=3)
        stderr.extend(tail)
        assert proc.returncode == 0, stderr
        assert b"\x1b[" not in stdout + stderr
        assert b"Input file:" in stderr, stderr
        deadline = time.monotonic() + 1
        while time.monotonic() < deadline and firmware.commands[-1] != b"\x32\xac\x04\x01":
            time.sleep(0.01)
        assert b"\x32\xac\x00\x33" in firmware.commands and firmware.commands[-1] == b"\x32\xac\x04\x01"
        with socket.socket() as client:
            client.settimeout(1)
            assert client.connect_ex(("127.0.0.1",port)) != 0, "API listener survived process exit"
    finally:
        if proc.poll() is None:
            proc.kill(); proc.wait()
        if firmware is not None:
            firmware.close()
        proc.stdout.close(); proc.stderr.close()

print("Progress input, JSON-RPC and serial smoke checks passed")
