#!/bin/sh
set -eu
bundle_root=${1:-"${TMPDIR:-/tmp}/goosic-debug-$(date +%Y%m%d-%H%M%S)"}
mkdir -p "$bundle_root"
git status --short >"$bundle_root/git-status.txt"
git diff --check >"$bundle_root/diff-check.txt" || true
sw_vers >"$bundle_root/macos-version.txt"
xcodebuild -version >"$bundle_root/xcode-version.txt"
make test-swift >"$bundle_root/swift-tests.txt" 2>&1 || true
archive="${bundle_root}.zip"
ditto -c -k --sequesterRsrc --keepParent "$bundle_root" "$archive"
printf '%s\n' "$archive"
