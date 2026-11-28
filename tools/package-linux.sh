#!/bin/sh
set -eu

# Build the release binary and stage a Linux install tree with the desktop entry, the icon,
# and the command that resets the settings.
repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
output=${1:-"$repo_root/target/package/linux"}

# Build for the explicit host target, so release output sits in target/<triple>/release
# like the Windows cross-build.
target=$(rustc -vV | sed -n 's/^host: //p')
cargo build --locked --release --target "$target" \
    --manifest-path "$repo_root/Cargo.toml" --bin texcel
binary="$repo_root/target/$target/release/texcel"
mkdir -p "$output/usr/bin" \
    "$output/usr/share/applications" \
    "$output/usr/share/icons/hicolor/256x256/apps"
cp "$binary" "$output/usr/bin/texcel"
cp "$repo_root/packaging/linux/texcel-reset-settings" "$output/usr/bin/texcel-reset-settings"
cp "$repo_root/packaging/linux/net.homeunix.hio.app.Texcel.desktop" \
    "$output/usr/share/applications/net.homeunix.hio.app.Texcel.desktop"
cp "$repo_root/assets/icons/app_icon_linux_256.png" \
    "$output/usr/share/icons/hicolor/256x256/apps/texcel.png"
# Ship the license text of Texcel and of every crate built into it.
cargo run --locked --quiet --manifest-path "$repo_root/Cargo.toml" \
    --bin texcel-licenses -- "$output/usr/share/doc/texcel/licenses" --target "$target"
chmod 755 "$output/usr/bin/texcel" "$output/usr/bin/texcel-reset-settings"
echo "created $output"
