#!/bin/sh
set -eu

# Build one platform package and zip it with its licenses, the user manuals, and the script
# that resets the settings, as target/dist/Texcel-<platform>-<version>.zip. Both platforms
# are built on macOS.
repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
platform=${1:?usage: package-zip.sh mac|windows <version>}
version=${2:?usage: package-zip.sh mac|windows <version>}
name="Texcel-$platform-$version"
dist="$repo_root/target/dist"
stage="$dist/$name"

# Start from an empty folder so files from an earlier run never enter the zip.
rm -rf "$stage" "$dist/$name.zip"
mkdir -p "$stage"
case "$platform" in
    mac) sh "$repo_root/tools/package-macos.sh" "$stage" ;;
    windows) sh "$repo_root/tools/package-windows.sh" "$stage" ;;
    *)
        echo "unknown platform: $platform (use mac or windows)" >&2
        exit 1
        ;;
esac
cp "$repo_root/docs/user-manual.en.md" "$repo_root/docs/user-manual.ja.md" "$stage/"
# The script that deletes the settings file, for troubleshooting.
case "$platform" in
    mac) cp "$repo_root/packaging/macos/reset-settings.command" "$stage/" ;;
    windows) cp "$repo_root/packaging/windows/reset-settings.cmd" "$stage/" ;;
esac

# ditto keeps the app bundle and its signature intact; local extended attributes are left
# out so the zip has no __MACOSX folder. The Windows folder is a plain zip.
cd "$dist"
if [ "$platform" = mac ]; then
    ditto -c -k --norsrc --noextattr --noacl --keepParent "$name" "$name.zip"
else
    zip -q -r -X "$name.zip" "$name"
fi
echo "created $dist/$name.zip"
