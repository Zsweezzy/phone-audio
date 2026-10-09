#!/usr/bin/env bash
# Install phone-audio.
#
# Works from a release tarball (prebuilt binaries) or from a git checkout
# (builds release binaries first with cargo). Installs into ~/.local.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
prefix="${PREFIX:-$HOME/.local}"
bin_dir="$prefix/bin"
share_dir="$prefix/share"

usage() {
    echo "usage: $0" >&2
    exit 1
}

[ $# -eq 0 ] || usage

# Tarball layout: prebuilt binaries live next to this script.
if [ -x "$here/bin/phone-audio" ] && [ -x "$here/bin/phone-audio-gui" ]; then
    src_bin="$here/bin"
else
    # Repo layout: build from source.
    if [ ! -f "$here/Cargo.toml" ] || ! command -v cargo >/dev/null 2>&1; then
        echo "install.sh: no prebuilt binaries in $here and no Cargo.toml + cargo found" >&2
        exit 1
    fi
    echo "building release binaries…"
    cargo build --release --manifest-path "$here/Cargo.toml"
    src_bin="$here/target/release"
fi

mkdir -p "$bin_dir" "$share_dir/applications" "$share_dir/icons/hicolor/scalable/apps"

install -m 0755 "$src_bin/phone-audio" "$bin_dir/phone-audio"
install -m 0755 "$src_bin/phone-audio-gui" "$bin_dir/phone-audio-gui"

# Icon + desktop entry.
if [ -f "$here/assets/phone-audio.svg" ]; then
    install -m 0644 "$here/assets/phone-audio.svg" \
        "$share_dir/icons/hicolor/scalable/apps/phone-audio.svg"
fi
if [ -f "$here/phone-audio.desktop" ]; then
    install -m 0644 "$here/phone-audio.desktop" \
        "$share_dir/applications/phone-audio.desktop"
fi

update-desktop-database "$share_dir/applications" 2>/dev/null || true

echo "installed to $bin_dir (add it to PATH if needed); tray icon in $share_dir/applications"
echo "quickshell shim: cp $here/quickshell/PhoneAudio.qml ~/.config/quickshell/<your-config>/ (then use PhoneAudio.* in QML)"