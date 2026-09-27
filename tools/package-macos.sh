#!/bin/sh
# Builds Goosic.app for macOS and zips it, ready to hand to a tester.
#
# Usage: sh tools/package-macos.sh [version]
#
# The app carries everything it needs: the shell, the service beside it in Contents/MacOS, and the
# target's resource bundle in Contents/Resources, which is where GoosicResources looks first. Both
# halves are built for Apple silicon and Intel and joined, so one download runs on either Mac.
#
# It is signed ad hoc, with no Developer ID, because this is a build for people we hand it to
# directly rather than something distributed through the store. macOS will refuse to open it until
# the person allows it once; docs/RELEASING.md says how.
set -eu

test "$(uname -s)" = Darwin || { echo "macOS app bundles are built on macOS" >&2; exit 2; }

version=${1:-0.1.0-alpha.2}
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

scratch="$HOME/Library/Caches/goosic-swift-build"
dist="$root/dist"
app="$dist/Goosic.app"
name="Goosic-$version-macos-universal"

rm -rf "$app" "$dist/$name.zip"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"

cargo build --release --target aarch64-apple-darwin --target x86_64-apple-darwin -p goosic-service
lipo -create -output "$app/Contents/MacOS/goosic-service" \
    "target/aarch64-apple-darwin/release/goosic-service" \
    "target/x86_64-apple-darwin/release/goosic-service"

SCUI_DEFAULT_BACKEND=AppKitBackend swift build -c release \
    --package-path apps/goosic-swift --scratch-path "$scratch" \
    --arch arm64 --arch x86_64
built=$(SCUI_DEFAULT_BACKEND=AppKitBackend swift build -c release \
    --package-path apps/goosic-swift --scratch-path "$scratch" \
    --arch arm64 --arch x86_64 --show-bin-path)
cp "$built/goosic-swift" "$app/Contents/MacOS/Goosic"
cp -R "$built/goosic-swift_GoosicSwift.bundle" "$app/Contents/Resources/"

# The Finder and Dock icon. The artwork bundled for the shell is one square PNG, which `iconutil`
# turns into the icon set macOS expects.
iconset=$(mktemp -d)/Goosic.iconset
mkdir -p "$iconset"
artwork="apps/goosic-swift/Sources/GoosicSwift/Resources/AppIcons/Icon-iOS-Default-1024@1x.png"
for size in 16 32 64 128 256 512; do
    sips -z $size $size "$artwork" --out "$iconset/icon_${size}x${size}.png" >/dev/null
    sips -z $((size * 2)) $((size * 2)) "$artwork" --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/Goosic.icns"

cat >"$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>Goosic</string>
    <key>CFBundleDisplayName</key><string>Goosic</string>
    <key>CFBundleExecutable</key><string>Goosic</string>
    <key>CFBundleIdentifier</key><string>io.github.analogicgoose.Goosic</string>
    <key>CFBundleIconFile</key><string>Goosic</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>${version%%-*}</string>
    <key>CFBundleVersion</key><string>$version</string>
    <key>LSMinimumSystemVersion</key><string>14.0</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSHumanReadableCopyright</key><string>GPL-3.0</string>
</dict>
</plist>
PLIST

codesign --force --deep --sign - "$app"
codesign --verify --deep --strict "$app"

ditto -c -k --keepParent "$app" "$dist/$name.zip"
echo "Packaged $dist/$name.zip"
