# Glance

<p align="center">
  <img src="assets/glance-logo.png" alt="Glance logo: a G made of LED dots with a mint-green accent" width="320">
</p>

Glance composes widgets on a Framework Laptop 16 LED Matrix. Preview the 9×34
output in a terminal editor, position widgets without overlap, save JSON layouts,
and play them on a device with or without the TUI.

The catalog includes Snake, Ledris, Pong, Game of Life, clocks (including a compact
seven-segment clock), computer CPU/RAM statistics, equalizer, breathing, WPM and
configurable progress bars. Progress bars consume values from periodically read
files with JSON Pointer/regex masks, or from one shared Axum JSON-RPC server.

## Getting started

Install Rust 1.95 or newer, [just](https://github.com/casey/just), and your system's
serial dependencies (`libudev-dev` on Ubuntu; `libudev` and `pkg-config` on Arch).
This workspace builds directly for your computer's native target.

```sh
just demo
just tui --serial-dev /dev/ttyACM0
just devices
just demo --template inputmodule-tui/templates/progress.json
just template inputmodule-tui/templates/progress.json --serial-dev /dev/ttyACM0
```

The editor needs a terminal of at least 64×24. Enter adds/positions a widget,
Tab switches lists, `e` configures a placed progress bar, `s` saves, and `q` exits.
The [complete guide](inputmodule-tui/README.md) covers keyboard/mouse controls,
template validation, file inputs, JSON-RPC methods and reconnection behavior.

Plain Cargo also works:

```sh
cargo run -- --no-device
cargo build --locked --workspace
cargo test --locked --workspace
```

The `inputmodule-control` binary provides the original hardware control commands:

```sh
just control --help
```

## Linux device permissions

The original udev rules are included in `release/50-framework-inputmodule.rules`.
Install them once, then reconnect the module:

```sh
sudo install -m 0644 release/50-framework-inputmodule.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules
```

## Workspace and original library

| Crate | Responsibility |
| --- | --- |
| `inputmodule-tui` | Widget editor, rendering, JSON templates, inputs, JSON-RPC server and device connection |
| `inputmodule-control` | Hardware control CLI |
| `inputmodule-host` | Shared protocol constants/encoding, monochrome grid and fonts |

The project was extracted from our working copy of
[FrameworkComputer/inputmodule-rs](https://github.com/FrameworkComputer/inputmodule-rs),
based on commit `4871f4ad71d64189bfafebae3a13e5a517cf3787` plus the local Rust
widget/editor improvements. Source files for both applications and the shared
library were preserved during extraction. The original repository remains intact.

`inputmodule-host` is temporarily included because this lightweight crate is not
yet published in the upstream repository. A fresh clone of Glance has everything
needed to build. The desktop crates reference this library through
`[workspace.dependencies]` in the root Cargo.toml. Once a repository publishes
that crate, the dependency can be replaced there with a pinned Git dependency;
application source imports remain the same. At that point, remove the bundled
crate from workspace members and remove its directory.

The workspace contains desktop applications and this shared library. It has no
RP2040 firmware crates or embedded-target configuration. Optional audio
visualizations in the control CLI retain their original feature flag and Git
revision.

## Verification

```sh
just check
just smoke
```

`check` runs formatting, Clippy and tests for all three crates. The four Linux
smoke suites exercise real terminal lifecycle, editor configuration/save/load,
file polling, the HTTP JSON-RPC endpoint, simulated serial output, signal shutdown
and reconnects. CI runs desktop checks on Linux, Windows and macOS, with integration
checks on Linux.

Glance is licensed under the [MIT License](LICENSE). The original Framework
copyright notice is retained.
