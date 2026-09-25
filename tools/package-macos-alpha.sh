#!/bin/sh
# Build a self-contained macOS alpha archive. The Sparkle archive signature and appcast are
# generated after this step, from the exact archive that will be published.
set -eu

test "$(uname -s)" = Darwin || { echo "Run this on macOS" >&2; exit 2; }
test "$#" -eq 2 || { echo "Usage: sh tools/package-macos-alpha.sh <version> <build-number>" >&2; exit 2; }
version=$1
build_number=$2
case "$version" in *[!0-9.]*|'') echo "Version must be numeric dotted text" >&2; exit 2;; esac
case "$build_number" in *[!0-9]*|'') echo "Build number must be an integer" >&2; exit 2;; esac

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
scratch=${GOOSIC_SWIFT_SCRATCH:-"$HOME/Library/Caches/goosic-swift-build"}
arches=${GOOSIC_MACOS_ARCHES:-arm64}
identity=${GOOSIC_MACOS_SIGNING_IDENTITY:--}
sdk=$(xcrun --show-sdk-version)
dist="$root/dist"
work=$(mktemp -d "${TMPDIR:-/tmp}/goosic-alpha.XXXXXX")
trap 'rm -rf "$work"' EXIT HUP INT TERM
app="$work/Goosic.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources" "$app/Contents/Frameworks" "$dist"

case "$arches" in
    arm64)
        cargo build --release -p goosic-service
        cp target/release/goosic-service "$app/Contents/MacOS/goosic-service"
        swift_arches='--arch arm64'
        archive_arch=arm64
        ;;
    universal)
        cargo build --release --target aarch64-apple-darwin --target x86_64-apple-darwin -p goosic-service
        lipo -create -output "$app/Contents/MacOS/goosic-service" \
            target/aarch64-apple-darwin/release/goosic-service \
            target/x86_64-apple-darwin/release/goosic-service
        swift_arches='--arch arm64 --arch x86_64'
        archive_arch=universal
        ;;
    *) echo "GOOSIC_MACOS_ARCHES must be arm64 or universal" >&2; exit 2;;
esac

# Splitting the known arch flags here is intentional; they are fixed strings from the case above.
# shellcheck disable=SC2086
SCUI_DEFAULT_BACKEND=AppKitBackend GOOSIC_MACOS_SDK="$sdk" swift build -c release \
    --package-path apps/goosic-swift --scratch-path "$scratch" $swift_arches
# shellcheck disable=SC2086
bin=$(SCUI_DEFAULT_BACKEND=AppKitBackend GOOSIC_MACOS_SDK="$sdk" swift build -c release \
    --package-path apps/goosic-swift --scratch-path "$scratch" $swift_arches --show-bin-path)
test -f "$bin/goosic-swift_GoosicSwift.bundle/Contents/Resources/PersonalCatalog.js" || {
    echo "The personal catalog resource is missing from the build" >&2; exit 1;
}
cp "$bin/goosic-swift" "$app/Contents/MacOS/Goosic"
ditto "$bin/goosic-swift_GoosicSwift.bundle" \
    "$app/Contents/Resources/goosic-swift_GoosicSwift.bundle"

framework="$scratch/artifacts/sparkle/Sparkle/Sparkle.xcframework/macos-arm64_x86_64/Sparkle.framework"
test -d "$framework" || { echo "The Sparkle framework is missing from the build" >&2; exit 1; }
ditto "$framework" "$app/Contents/Frameworks/Sparkle.framework"
if ! otool -l "$app/Contents/MacOS/Goosic" | grep -q '@executable_path/../Frameworks'; then
    install_name_tool -add_rpath '@executable_path/../Frameworks' "$app/Contents/MacOS/Goosic"
fi

iconset="$work/Goosic.iconset"
mkdir -p "$iconset"
artwork="apps/goosic-swift/Sources/GoosicSwift/Resources/AppIcons/Icon-iOS-Default-1024@1x.png"
for size in 16 32 64 128 256 512; do
    sips -z "$size" "$size" "$artwork" --out "$iconset/icon_${size}x${size}.png" >/dev/null
    double=$((size * 2))
    sips -z "$double" "$double" "$artwork" --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/Goosic.icns"

cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
    <key>CFBundleName</key><string>Goosic</string>
    <key>CFBundleDisplayName</key><string>Goosic</string>
    <key>CFBundleExecutable</key><string>Goosic</string>
    <key>CFBundleIdentifier</key><string>io.github.analogicgoose.Goosic</string>
    <key>CFBundleIconFile</key><string>Goosic</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$version</string>
    <key>CFBundleVersion</key><string>$build_number</string>
    <key>LSMinimumSystemVersion</key><string>14.0</string>
    <key>SUFeedURL</key><string>https://raw.githubusercontent.com/AnalogicGoose/GoosicReborn/development/updates/macos-alpha.xml</string>
    <key>SUPublicEDKey</key><string>Ci4/2HQuY9YM4a+c4L/gE5soALNo6edEJvPDZHzRMzM=</string>
    <key>SUVerifyUpdateBeforeExtraction</key><true/>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSHumanReadableCopyright</key><string>GPL-3.0</string>
</dict></plist>
PLIST
cp LICENSE-GPL-3.0 "$app/Contents/Resources/LICENSE-GPL-3.0"

if test "$identity" = -; then
    codesign --force --deep --sign - "$app"
else
    codesign --force --deep --options runtime --timestamp --sign "$identity" "$app"
fi
codesign --verify --deep --strict --verbose=2 "$app"

name="Goosic-$version-macos-alpha-$archive_arch.zip"
ditto -c -k --keepParent "$app" "$dist/$name"
shasum -a 256 "$dist/$name" > "$dist/SHA256SUMS-macos.txt"
echo "Packaged $dist/$name"
