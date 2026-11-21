#!/bin/sh
set -eu

# Build the release binary for Apple silicon and Intel Macs, join them into one universal
# binary, bundle it with its ICNS icon as Texcel.app, and put the licenses folder beside
# the app, where users can open it as on the other platforms.
repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
output=${1:-"$repo_root/target/package/mac"}
app="$output/Texcel.app"
contents="$app/Contents"
targets="aarch64-apple-darwin x86_64-apple-darwin"

if [ ! -f "$repo_root/assets/icons/app_icon.icns" ]; then
    echo "missing app_icon.icns; run the documented icon converter first" >&2
    exit 1
fi

# Build each architecture with an explicit target, so release output sits in
# target/<triple>/release like the Windows cross-build. Missing Rust targets are added
# when rustup manages the toolchain.
binaries=""
for target in $targets; do
    if command -v rustup >/dev/null 2>&1 \
        && ! rustup target list --installed | grep -qx "$target"; then
        rustup target add "$target"
    fi
    cargo build --locked --release --target "$target" \
        --manifest-path "$repo_root/Cargo.toml" --bin texcel
    binaries="$binaries $repo_root/target/$target/release/texcel"
done

# Start from an empty bundle so files from an earlier layout never stay inside it.
rm -rf "$app" "$output/licenses"
mkdir -p "$contents/MacOS" "$contents/Resources"
# shellcheck disable=SC2086 # one path per architecture, none with spaces in target/
lipo -create -output "$contents/MacOS/Texcel" $binaries
cp "$repo_root/assets/icons/app_icon.icns" "$contents/Resources/app_icon.icns"
cp "$repo_root/packaging/macos/Info.plist" "$contents/Info.plist"

# Ship the license text of Texcel and of every crate built into either architecture.
# The x86_64 build pulls in a few crates the arm64 build does not, so the folders are
# merged; a file that exists in both must be identical, or the merge would hide one.
scratch=$(mktemp -d "${TMPDIR:-/tmp}/texcel-licenses.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
for target in $targets; do
    cargo run --locked --quiet --manifest-path "$repo_root/Cargo.toml" \
        --bin texcel-licenses -- "$scratch/$target" --target "$target"
done
if diff -rq "$scratch/aarch64-apple-darwin" "$scratch/x86_64-apple-darwin" \
    | grep -v '^Only in ' | grep -q .; then
    echo "the arm64 and x86_64 license folders disagree on a shared file" >&2
    exit 1
fi
mkdir -p "$output/licenses"
for target in $targets; do
    cp -R "$scratch/$target/." "$output/licenses/"
done

chmod 755 "$contents/MacOS/Texcel"
codesign --force --sign - --timestamp=none "$app"
codesign --verify --strict "$app"
echo "created $app"
