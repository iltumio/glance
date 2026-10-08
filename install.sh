#!/usr/bin/env bash
set -euo pipefail

fail() {
    printf 'glance: %s\n' "$*" >&2
    exit 1
}

cleanup() {
    if [[ -n "${glance_temp_dir:-}" ]]; then
        rm -rf -- "$glance_temp_dir"
    fi
}

main() {
    local glance_prefix="${HOME}/.local"
    local glance_source_dir glance_script_path glance_host_target glance_target_dir
    local glance_install_dir glance_command

    while [[ $# -gt 0 ]]; do
        case "$1" in
            --prefix)
                [[ $# -ge 2 && -n "$2" ]] || fail '--prefix requires a directory.'
                glance_prefix="$2"
                shift 2
                ;;
            -h|--help)
                printf 'Usage: install.sh [--prefix DIRECTORY]\n\nBuild Glance and install its binaries into DIRECTORY/bin (default: ~/.local/bin).\nRequires Rust 1.95+, a C compiler, and Linux libudev/pkg-config.\n'
                return
                ;;
            *) fail "Unknown option: $1 (use --help)." ;;
        esac
    done

    case "$(uname -s)" in
        Linux)
            command -v pkg-config >/dev/null || fail 'Install pkg-config before running the installer.'
            pkg-config --exists libudev || fail 'Install libudev development files (libudev-dev on Ubuntu; libudev on Arch).'
            ;;
        Darwin)
            xcode-select -p >/dev/null 2>&1 || fail 'Install the Xcode Command Line Tools with: xcode-select --install'
            ;;
        *) fail 'This installer supports Linux and macOS. On Windows, build with cargo build --locked --release --workspace.' ;;
    esac
    for glance_command in cargo rustc install; do
        command -v "$glance_command" >/dev/null || fail "Missing $glance_command. Install Rust 1.95+ and your platform's build tools."
    done

    glance_script_path="${BASH_SOURCE[0]:-}"
    glance_source_dir=''
    if [[ -f "$glance_script_path" ]]; then
        glance_source_dir="$(cd -- "$(dirname -- "$glance_script_path")" && pwd)"
    fi
    if [[ ! -f "$glance_source_dir/inputmodule-tui/Cargo.toml" ]]; then
        for glance_command in curl tar; do
            command -v "$glance_command" >/dev/null || fail "Missing $glance_command, needed to download the source."
        done
        glance_temp_dir="$(mktemp -d "${TMPDIR:-/tmp}/glance-install.XXXXXX")"
        trap cleanup EXIT
        trap 'exit 130' INT
        trap 'exit 143' TERM
        printf 'Downloading Glance source from GitHub...\n'
        curl -fL --retry 3 https://github.com/iltumio/glance/archive/refs/heads/main.tar.gz \
            -o "$glance_temp_dir/source.tar.gz"
        glance_source_dir="$glance_temp_dir/source"
        mkdir -p -- "$glance_source_dir"
        tar -xzf "$glance_temp_dir/source.tar.gz" --strip-components=1 -C "$glance_source_dir"
    fi

    glance_host_target="$(cd -- "$glance_source_dir" && rustc -vV | sed -n 's/^host: //p')"
    [[ -n "$glance_host_target" ]] || fail 'Could not determine the native Rust target.'
    glance_target_dir="${CARGO_TARGET_DIR:-$glance_source_dir/target}"
    mkdir -p -- "$glance_target_dir"
    glance_target_dir="$(cd -- "$glance_target_dir" && pwd)"
    printf 'Building Glance for %s...\n' "$glance_host_target"
    (
        cd -- "$glance_source_dir"
        cargo build --locked --release --workspace --target "$glance_host_target" \
            --target-dir "$glance_target_dir"
    )

    glance_install_dir="$glance_prefix/bin"
    mkdir -p -- "$glance_install_dir"
    glance_install_dir="$(cd -- "$glance_install_dir" && pwd)"
    install -m 755 "$glance_target_dir/$glance_host_target/release/inputmodule-tui" "$glance_install_dir/"
    install -m 755 "$glance_target_dir/$glance_host_target/release/inputmodule-control" "$glance_install_dir/"
    printf 'Installed inputmodule-tui and inputmodule-control in %s\n' "$glance_install_dir"
    case ":${PATH:-}:" in
        *":$glance_install_dir:"*) ;;
        *) printf 'Add this directory to your PATH:\n  export PATH=%q:%s\n' "$glance_install_dir" "\$PATH" ;;
    esac
    printf 'Try: inputmodule-tui --no-device\n'
}

main "$@"
