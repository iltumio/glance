# Framework Input Module widget editor

Compose games, clocks, animations and live CPU/RAM statistics on a Framework
Laptop 16 LED Matrix. The TUI shows the live 9x34 output on the left and the widget
catalog and placed widgets on the right. Save the layout as JSON and play it
without a TUI using the same rendering engine. Every catalog widget also works
without a physical matrix.

## Build and run

Install Rust 1.95 or newer and the native serial dependencies (`libudev-dev` on
Ubuntu, `libudev` and `pkg-config` on Arch). This desktop workspace uses your computer's native target. The
[just](https://github.com/casey/just) recipes run directly on the host:

```sh
just demo                              # Widget editor without hardware
just tui --serial-dev /dev/ttyACM0      # Widget editor with a chosen matrix
just devices                           # List compatible matrices
just tui --save-template layout.json   # Choose the editor's save destination
just tui --template layout.json        # Load and edit an existing layout
just template layout.json              # Play without a TUI
just validate-template layout.json     # Validate without hardware or a terminal
just check
just smoke                             # Linux only; no physical hardware needed
```

If only one compatible matrix is connected, `just tui` finds it automatically.
When several are connected, select one with `--serial-dev`. Explicit paths can
also be `/dev/serial/by-id/...` or `COM3`. For Linux permissions, install the udev
rule described in the root README.

Direct Cargo commands are available:

```sh
cargo run -p inputmodule-tui -- --no-device
cargo run -p inputmodule-tui -- --template layout.json --headless
```

## Compose a view

The editor needs a terminal of at least **64x24**. At this size, each terminal
cell uses Braille dots to represent two LED rows; taller terminals display
individual white LED dots, with dim dots marking unlit LEDs.
The gray background in the preview marks the selected widget's complete footprint
and is only an editing aid. The physical matrix receives the actual lit pixels.

1. Select a widget in **Add widgets** with Up/Down and press Enter, or click an
   enabled row. It is added at the first free position that fits its rectangle.
2. Move it with arrows or `hjkl`, then press Enter to confirm. A click or drag on
   the preview positions its top-left corner. In a compact preview mouse positioning
   advances two LED rows; arrows always move one LED. Escape cancels a new addition
   or restores the original position of an existing widget.
3. Press Tab to switch between the catalog and **Placed widgets**. In the placed
   list, Enter moves the selected widget; Delete or `d` removes it.
4. For a placed Progress Bar, press `e` to edit its name, dimensions, range and
   input. Up/Down or Tab switches fields, Delete clears a field, typing/Backspace
   edits it, Enter applies and Escape cancels. Invalid settings or a resized
   footprint that overlaps/leaves the canvas keep the form open without changing
   the widget. Press `p` to send keyboard input to the selected widget. Escape returns to
   editing. Games can restart with `r`; WPM counts ordinary letters, including `r`.
5. Press `s` to save. The destination is `--save-template`, otherwise the loaded
   template, otherwise `layout.json`. A `*` marks edits since the last successful
   save. Saving replaces the destination atomically; failures remain visible.

`q` or Escape exits the editor outside placement/input/configuration mode. Ctrl-C exits from
all modes. Edits are only saved when `s` is pressed. Saving during placement
confirms the position first.

Widgets cannot overlap or leave the 9x34 canvas. Gray **[no room]** entries cannot
be added with Enter or a mouse click. Availability considers the size of a
contiguous free rectangle, rather than just the total number of unused LEDs.
Moving/removing widgets immediately updates the available options. Multiple
instances of a widget can be added when they fit.
The catalog and placed list scroll automatically to keep the selected row visible.

## Widget catalog

Dimensions are in LEDs and include the whole reserved footprint.

| Widget | Width × height | Behavior |
| --- | --- | --- |
| Snake | 9 × 34 | Arrows or `hjkl`; filling the board wins |
| Ledris | 9 × 34 | Left/Right move, Up rotates, Down drops, Space hard drops |
| Pong | 9 × 34 | Host-rendered; Left/Right top paddle, `a`/`d` bottom paddle |
| Game of Life | 9 × 12 | Host-rendered; `n` in input mode cycles the initial pattern |
| System Monitor | 9 × 12 | Computer CPU left, RAM right; sampled once per second |
| Clock | 5 × 34 | Local 24-hour time in vertically stacked characters |
| Clock (7-segment) | 7 × 11 | Hours above minutes, one blank row, no colon |
| Equalizer | 9 × 10 | Random centered bars; not an audio analyzer |
| Breathing | 9 × 6 | Animated local pixel density; does not alter global brightness |
| WPM | 5 × 34 | Counts typing in input mode over a rolling ten-second window |
| Progress Bar | Configurable; default 3 × 1 | Manual value, file polling or JSON-RPC input |

CPU requires an initial warm-up; missing metrics are labelled in the terminal.
RAM is shown as a percentage and used/total GiB. System monitoring refreshes only
CPU and RAM, without scanning processes, disks or networks.

Pong uses automatic paddles while editing and in headless playback. In input mode,
the keyboard controls the paddles. Snake and Ledris run in the preview and restart
after game over outside input mode; in input mode, game over remains until `r`.
WPM receives keys only in TUI input mode; headless playback does not capture the
computer's global keyboard input. Game of Life uses finite, non-wrapping edges.

## JSON templates and headless playback

An included layout combines the compact clock, CPU/RAM and equalizer:

```sh
just tui --template inputmodule-tui/templates/clock-stats.json
just template inputmodule-tui/templates/clock-stats.json --serial-dev /dev/ttyACM0
```

The template stores placements and configuration, not game state or old samples:

```json
{
  "version": 1,
  "canvas": { "width": 9, "height": 34 },
  "widgets": [
    { "kind": "seven_segment_clock", "x": 1, "y": 0 },
    { "kind": "system_monitor", "x": 0, "y": 11 },
    { "kind": "equalizer", "x": 0, "y": 23 }
  ]
}
```

Coordinates start at the top-left `(0, 0)`. Sizes are supplied by the catalog.
Supported kinds are `snake`, `ledris`, `pong`, `game_of_life`, `system_monitor`,
`clock`, `seven_segment_clock`, `equalizer`, `breathing`, `wpm` and `progress_bar`.

Game of Life accepts an optional `pattern`: `glider` (default), `blinker`, `toad`,
`beacon`, `beacon_toad_blinker`, `random` or `easter_egg`. Example:

```json
{ "kind": "game_of_life", "x": 0, "y": 12, "pattern": "blinker" }
```

Unknown fields/kinds, unsupported versions, incorrect canvas dimensions,
out-of-bounds placements, overlaps and inappropriate options are rejected before
opening a terminal or connecting to the device. Templates are limited to 1 MiB.
`--validate-template` validates and exits, without creating widget runtime state.

Headless playback never enables terminal raw mode or an alternate screen. Ctrl-C
stops it and restores the device settings. SIGTERM also exits cleanly on Unix.
For a bounded run or a hardware-free check:

```sh
just template layout.json --no-device --duration 5
```

The program must stay running to update the matrix; templates do not require
changes to the device firmware.

## Progress bars and external inputs

Add **Progress Bar**, place it, then press `e` in the placed list. Its default
footprint is the minimum **3 LEDs long and 1 LED wide**. `length` is the number
of LEDs along the fill direction; `width` is the thickness. Horizontal bars can
be up to 9 LEDs long and 34 thick; vertical bars up to 34 long and 9 thick, always
subject to free space and placement. Catalog availability uses the initial 3x1
footprint; resizing checks the complete configured rectangle.

`orientation` is `horizontal` or `vertical`. Horizontal bars fill left to right;
vertical bars fill bottom to top. `reverse: true` reverses that direction. Every
filled segment lights the whole thickness. Filled length is rounded down to
whole LEDs, with all LEDs lit at 100%. `min`/`max` define a finite numeric range
(default 0..100); `value` is the initial/manual value (default 0). Values outside
the range are clamped for rendering. Current input samples are not saved in JSON.

The included layout contains a horizontal API bar named `download`, a vertical
file bar named `build`, and another vertical API bar named `tests`:

```sh
just demo --template inputmodule-tui/templates/progress.json
# Or play on the device without TUI:
just template inputmodule-tui/templates/progress.json --serial-dev /dev/ttyACM0
```

An individual placement can be written as:

```json
{
  "kind": "progress_bar", "name": "build", "x": 0, "y": 5,
  "progress": {
    "orientation": "vertical", "length": 24, "width": 2,
    "min": 0, "max": 100, "value": 0, "reverse": false
  },
  "input": {
    "type": "file", "path": "progress-data.json", "interval_ms": 1000,
    "mask": { "type": "json_pointer", "pointer": "/build/percent" }
  }
}
```

Omit `input` for a manual bar. Names must be unique and contain 1–64 ASCII
letters, digits, underscores or hyphens; an API widget requires a name. New bars
in the TUI receive an unused `progress-N` name. These input definitions are
separate from drawing; Progress Bar is the first widget to consume them. Other
widget kinds currently reject external input options.

### File input

Paths are relative to the editor's template destination, or absolute. Polling
uses one background worker for all file inputs, without blocking the TUI or
serial updates. `interval_ms` defaults to 1000 and accepts 100..86400000. Files
must be regular UTF-8 files, at most 1 MiB. Supported masks:

- `{"type":"number"}` (default): the whole file is a number such as `42.5`.
- `{"type":"json_pointer","pointer":"/build/percent"}`: select a number or
  numeric string from JSON. An empty pointer selects the root; `/jobs/0/percent`
  selects an array element; `~0` and `~1` escape `~` and `/` in property names.
- `{"type":"regex","pattern":"progress=([0-9.]+)%","group":1}`: parse the
  selected capture from the first match. Capture 1 is the default, 0 is the
  entire match. Patterns and capture indexes are validated before playback.

Unreadable files, missing fields/matches, invalid UTF-8/JSON/numbers and nonfinite
values retain the last good value (or the initial value). The TUI shows input
errors in the selected widget's status; headless playback logs them on change.
Reading resumes automatically once the file becomes valid. For the demo, update
`inputmodule-tui/templates/progress-data.json` with `{"build":{"percent":80}}`.

### Shared JSON-RPC API

One Axum server starts when a layout contains an API input, including a bar
configured while the TUI is running. It listens on **127.0.0.1:3030**, with a
single `POST /rpc` endpoint. The server remains available when widgets are
removed, and stops with the application. Set the global address once with
`--api-listen`; an explicit address also starts the server before any API widget
is added:

```sh
just template inputmodule-tui/templates/progress.json --no-device --api-listen 127.0.0.1:8080
```

Change a named bar using JSON-RPC 2.0 `widget.update`:

```sh
curl http://127.0.0.1:3030/rpc \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"widget.update","params":{"widget":"download","values":{"value":75}}}'
```

Successful response:

```json
{"jsonrpc":"2.0","id":1,"result":{"widget":"download","values":{"value":75.0}}}
```

`widget.get` takes `{"widget":"download"}` and returns the latest accepted
value. `widget.list` takes no parameters and lists the names accepting API input.
Only API-configured widgets accept updates; file/manual bars cannot be overridden.
Progress Bar accepts a finite numeric `value`; extra value fields are rejected.
The response acknowledges the accepted input; rendering follows on the next
application update, normally within 100 ms. Repeated updates replace a bounded
latest-value slot, rather than accumulating a queue.

The endpoint follows the [JSON-RPC 2.0 specification](https://www.jsonrpc.org/specification)
for request IDs, batch requests, notifications, and standard error codes.
Notifications (requests without `id`) produce HTTP 204 with no body; malformed
JSON produces `-32700`, invalid requests `-32600`, unknown methods `-32601`,
invalid parameters `-32602`, and unknown widget names `-32001`. Request bodies
are limited to 64 KiB. The loopback server requires no authentication.

## Device behavior

The editor and headless player retry a disconnected device every two seconds,
without resetting widget state. Host rendering disables scrolling and stops any
firmware game. Original brightness and scrolling settings are queried and restored
on exit, including errors and unwinding panics while the serial port is usable.
Force-killing or unplugging the module prevents restoration.

Only changed frames are sent. A thirty-second keepalive prevents the firmware's
sixty-second inactivity timeout from putting unchanged output to sleep.

## Standalone examples

The original fullscreen examples menu remains available:

```sh
just examples --no-device
just examples --serial-dev /dev/ttyACM0
```

Up/Down selects; Enter starts; `q`/Escape returns to the menu (Escape only for WPM),
and Ctrl-C quits. In this mode Pong and Game of Life still use the device firmware
and require a matrix; Breathing changes the device's global brightness. The
original games, score overlay and game-over flashes remain available here.

## Verification

`just check` runs formatting, Clippy, application tests and the shared library's
tests. CI builds/tests on Linux, Windows and macOS. Linux smoke tests exercise real terminal startup and cleanup,
keyboard/mouse input, progress configuration, file masks/polling, JSON-RPC
batches/notifications/errors, save/load, rejected placements/templates, simulated serial
frames, headless signal shutdown and reconnection:

```sh
just smoke
```

Shared command encoding, firmware enums, fonts and monochrome frames come from
the bundled `inputmodule-host` crate, extracted from the original repository.
See the root README for provenance. The application workspace compiles the shared
host library and desktop
applications.
