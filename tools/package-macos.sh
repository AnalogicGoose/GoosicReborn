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

version=${1:-0.2.6}
version=${version#v}
# Finder's bundle versions are numeric, even when the release tag is a prerelease.
bundle_version=${version%%-*}
bundle_version=${bundle_version%%+*}
if ! printf '%s\n' "$bundle_version" | /usr/bin/grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$'; then
    echo "Expected a semantic version such as v0.2.3-alpha.1" >&2
    exit 2
fi
build_version=${GOOSIC_MACOS_BUILD_VERSION:-$bundle_version}
if ! printf '%s\n' "$build_version" | /usr/bin/grep -Eq '^[0-9]+(\.[0-9]+){0,2}$'; then
    echo "Expected a numeric macOS build version" >&2
    exit 2
fi
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

# Keep the service's deployment floor aligned with the shell and bundle. Cross-target C
# dependencies also need the selected Xcode SDK rather than an implicit build-host SDK.
export MACOSX_DEPLOYMENT_TARGET=14.0
SDKROOT=${SDKROOT:-$(xcrun --sdk macosx --show-sdk-path)}
CC=${CC:-$(xcrun --sdk macosx --find clang)}
export SDKROOT CC
# Preserve linker output: stripping release proc-macro dylibs with the macOS 27 toolchain
# can produce a misaligned LINKEDIT string pool that rustc cannot load.
CARGO_PROFILE_RELEASE_STRIP=${CARGO_PROFILE_RELEASE_STRIP:-none}
export CARGO_PROFILE_RELEASE_STRIP

scratch="$HOME/Library/Caches/goosic-swift-build"
dist="$root/dist"
# Assemble outside file-provider directories: Finder can restore metadata after xattr clears
# it, invalidating an otherwise signed app while the ZIP is being written.
stage=$(mktemp -d "${TMPDIR:-/tmp}/goosic-package.XXXXXX")
trap 'rm -rf "$stage"' EXIT
app="$stage/Goosic.app"
name="Goosic-$version-macos-universal"

mkdir -p "$dist"
rm -f "$dist/$name.zip"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources" "$app/Contents/Frameworks"
cp LICENSE LICENSE-GPL-3.0 "$app/Contents/Resources/"

cargo build --release --target aarch64-apple-darwin --target x86_64-apple-darwin -p goosic-service
rust_target=${CARGO_TARGET_DIR:-target}
lipo -create -output "$app/Contents/MacOS/goosic-service" \
    "$rust_target/aarch64-apple-darwin/release/goosic-service" \
    "$rust_target/x86_64-apple-darwin/release/goosic-service"

GOOSIC_MACOS_SDK="$(xcrun --sdk macosx --show-sdk-version)" swift build -c release \
    --package-path apps/goosic-swift --scratch-path "$scratch" \
    --arch arm64 --arch x86_64
built=$(GOOSIC_MACOS_SDK="$(xcrun --sdk macosx --show-sdk-version)" swift build -c release \
    --package-path apps/goosic-swift --scratch-path "$scratch" \
    --arch arm64 --arch x86_64 --show-bin-path)
cp "$built/goosic-swift" "$app/Contents/MacOS/Goosic"
ditto "$built/Sparkle.framework" "$app/Contents/Frameworks/Sparkle.framework"
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
    <key>CFBundleShortVersionString</key><string>$bundle_version</string>
    <key>CFBundleVersion</key><string>$build_version</string>
    <key>SUFeedURL</key><string>https://raw.githubusercontent.com/AnalogicGoose/GoosicReborn/development/updates/macos-alpha.xml</string>
    <key>SUPublicEDKey</key><string>Ci4/2HQuY9YM4a+c4L/gE5soALNo6edEJvPDZHzRMzM=</string>
    <key>SUVerifyUpdateBeforeExtraction</key><true/>
    <key>LSMinimumSystemVersion</key><string>14.0</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSHumanReadableCopyright</key><string>GPL-3.0</string>
</dict>
</plist>
PLIST

# File-provider/Finder metadata is not part of the generated bundle and prevents signing.
xattr -cr "$app"
codesign --force --deep --sign - "$app"
codesign --verify --deep --strict "$app"

ditto -c -k --keepParent "$app" "$dist/$name.zip"
echo "Packaged $dist/$name.zip"
