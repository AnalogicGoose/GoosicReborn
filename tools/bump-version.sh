#!/bin/sh
# Moves every version default a release touches to a new version, in one step.
#
# Usage: sh tools/bump-version.sh <x.y.z>
#
# It rewrites the workspace version in Cargo.toml and refreshes Cargo.lock, the release
# workflow's default tag and notes file, the Windows and macOS packaging defaults, and the
# example tag in docs/RELEASING.md. Each edit is anchored to the line it belongs to rather than
# to the old version alone, because the wire protocol has its own version that happens to share
# numbers with a release and must not move with it. The README paragraph stays hand-written.
set -eu

new=${1:-}
if ! printf '%s\n' "$new" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$'; then
    echo "Usage: sh tools/bump-version.sh <x.y.z>" >&2
    exit 2
fi
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

old=$(sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\([^"]*\)"/\1/p' Cargo.toml)
if [ -z "$old" ]; then
    echo "Could not read the workspace version from Cargo.toml" >&2
    exit 1
fi
if [ "$old" = "$new" ]; then
    echo "Already at $new" >&2
    exit 1
fi

# Rewrites with a sed expression and fails if the file did not change, so a moved line is
# reported instead of silently skipped.
rewrite() {
    file=$1
    expr=$2
    tmp=$(mktemp)
    sed "$expr" "$file" > "$tmp"
    if cmp -s "$file" "$tmp"; then
        rm -f "$tmp"
        echo "No $old default found in $file" >&2
        exit 1
    fi
    cat "$tmp" > "$file"
    rm -f "$tmp"
}

o=$(printf '%s' "$old" | sed 's/\./\\./g')
rewrite Cargo.toml "/^\[workspace.package\]/,/^\[/s/^version = \"$o\"/version = \"$new\"/"
rewrite .github/workflows/release.yml "s/default: v$o\$/default: v$new/"
rewrite .github/workflows/release.yml "s|body_path: docs/RELEASE_$o\.md|body_path: docs/RELEASE_$new.md|"
rewrite apps/goosic-windows/build-installer.ps1 "s/\[string\]\$Version = '$o'/[string]\$Version = '$new'/"
rewrite apps/goosic-windows/package.ps1 "s/\[string\]\$Version = '$o'/[string]\$Version = '$new'/"
rewrite tools/package-macos.sh "s/^version=\${1:-$o}/version=\${1:-$new}/"
rewrite docs/RELEASING.md "s/^\`v$o\` and push the tag/\`v$new\` and push the tag/"

# Only the workspace's own packages move; nothing is upgraded.
cargo update --workspace --offline >/dev/null 2>&1 || cargo update --workspace

echo "Moved $old to $new."
if [ ! -f "docs/RELEASE_$new.md" ]; then
    echo "docs/RELEASE_$new.md does not exist yet; the release workflow publishes it as the notes." >&2
    exit 1
fi
echo "Lines that still mention $old (the README and the protocol version are expected):"
git grep -n -F "$old" -- ':!docs/RELEASE_*.md' || true
