set positional-arguments

target_dir := env_var_or_default('CARGO_TARGET_DIR', 'target')

default:
    @just --list

# Open the widget editor; forward all CLI arguments.
tui *args:
    cargo run --locked -p inputmodule-tui -- "$@"

# Preview widgets without hardware.
demo *args:
    cargo run --locked -p inputmodule-tui -- --no-device "$@"

# List compatible matrices.
devices:
    cargo run --locked -p inputmodule-tui -- --list

# Play a JSON template without TUI.
template file *args:
    cargo run --locked -p inputmodule-tui -- --headless --template "$@"

# Validate a template without hardware or a terminal.
validate-template file:
    cargo run --locked -p inputmodule-tui -- --template "$1" --validate-template

# Open the original fullscreen demos menu.
examples *args:
    cargo run --locked -p inputmodule-tui -- --examples "$@"

# Run the hardware control CLI.
control *args:
    cargo run --locked -p inputmodule-control -- "$@"

# Build both desktop applications.
build *args:
    cargo build --locked --workspace "$@"

# Build optimized desktop applications.
release *args:
    cargo build --locked --release --workspace "$@"

# Run application tests.
test *args:
    cargo test --locked --workspace "$@"

# Run only the shared library tests.
test-lib:
    cargo test --locked -p inputmodule-host

# Lint applications; warnings are errors.
lint:
    cargo clippy --locked --workspace --all-targets -- -D warnings

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

# Verify applications and the shared library.
check: fmt-check lint test

# Test Linux terminal, HTTP API and simulated serial interactions.
[linux]
smoke: build
    python3 inputmodule-tui/tests/terminal_smoke.py {{ quote(target_dir / 'debug' / 'inputmodule-tui') }}
    python3 inputmodule-tui/tests/serial_smoke.py {{ quote(target_dir / 'debug' / 'inputmodule-tui') }}
    python3 inputmodule-tui/tests/composer_smoke.py {{ quote(target_dir / 'debug' / 'inputmodule-tui') }}
    python3 inputmodule-tui/tests/inputs_smoke.py {{ quote(target_dir / 'debug' / 'inputmodule-tui') }}
