#!/bin/sh
# Build a manual-install experiment beside the regular Goosic app, with no update feed.
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
version=${1:-0.4.1-macos-motion.1}
case "$version" in
    *-macos-motion.*) ;;
    *) echo "Expected a version such as 0.4.1-macos-motion.1" >&2; exit 2 ;;
esac
sh tools/package-macos.sh "$version"
archive="$root/dist/Goosic-$version-macos-universal.zip"
stage=$(mktemp -d "${TMPDIR:-/tmp}/goosic-motion-package.XXXXXX")
trap 'rm -rf "$stage"' EXIT
unzip -q "$archive" -d "$stage"
app="$stage/Goosic Motion Experiment.app"
mv "$stage/Goosic.app" "$app"
plist="$app/Contents/Info.plist"
/usr/libexec/PlistBuddy -c 'Delete :SUFeedURL' "$plist"
/usr/libexec/PlistBuddy -c 'Delete :SUPublicEDKey' "$plist"
/usr/libexec/PlistBuddy -c 'Set :CFBundleName Goosic Motion Experiment' "$plist"
/usr/libexec/PlistBuddy -c 'Set :CFBundleDisplayName Goosic Motion Experiment' "$plist"
/usr/libexec/PlistBuddy -c 'Set :CFBundleIdentifier io.github.analogicgoose.Goosic.MotionExperiment' "$plist"
xattr -cr "$app"
codesign --force --deep --sign - "$app"
codesign --verify --deep --strict "$app"
rm "$archive"
ditto -c -k --keepParent "$app" "$archive"
shasum -a 256 "$archive" > "$archive.sha256"
printf 'Packaged manual-install experiment: %s\n' "$archive"
