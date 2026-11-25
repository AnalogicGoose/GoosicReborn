#!/bin/sh
# Builds the Goosic Flatpak for Linux and exports it as a single-file bundle a tester installs.
#
# Usage: sh tools/package-linux-flatpak.sh [version]
#
# The version defaults to the workspace version in Cargo.toml; a prerelease tag such as
# v0.4.3-linux-alpha.1 is passed explicitly. The result is
# dist/Goosic-<version>-linux-<arch>.flatpak and dist/SHA256SUMS-linux.txt, where <arch> is the
# Flatpak architecture of this machine (x86_64 or aarch64) unless GOOSIC_FLATPAK_ARCH says
# otherwise.
#
# flatpak-builder fetches the sources the manifest names, including every crate listed in the
# two cargo-sources files, and then builds inside a sandbox with no network, so cargo runs
# --offline. When either Cargo.lock changes, regenerate the matching cargo-sources file first;
# docs/LINUX_SHELL.md says how. The bundle points at Flathub for the GNOME runtime, so installing
# it on a machine without the runtime fetches it from there.
set -eu

test "$(uname -s)" = Linux || { echo "Flatpak bundles are built on Linux" >&2; exit 2; }
for tool in flatpak flatpak-builder sha256sum; do
    command -v "$tool" >/dev/null 2>&1 || { echo "$tool is required" >&2; exit 2; }
done

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

default_version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)
version=${1:-$default_version}
version=${version#v}
if ! printf '%s\n' "$version" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+([-+][0-9A-Za-z.-]+)?$'; then
    echo "Expected a semantic version such as v0.4.3-linux-alpha.1" >&2
    exit 2
fi
arch=${GOOSIC_FLATPAK_ARCH:-$(flatpak --default-arch)}

app_id=io.github.analogicgoose.Goosic
manifest="$root/apps/goosic-linux/packaging/flatpak/$app_id.yml"
dist="$root/dist"
name="Goosic-$version-linux-$arch"
# The download cache survives between runs so a rebuild does not fetch every crate again. It
# lives outside the source tree, which the manifest copies into the sandbox as a whole.
state="${XDG_CACHE_HOME:-$HOME/.cache}/goosic-flatpak-builder"
stage=$(mktemp -d "${TMPDIR:-/tmp}/goosic-flatpak.XXXXXX")
trap 'rm -rf "$stage"' EXIT

mkdir -p "$dist" "$state"
rm -f "$dist/$name.flatpak"

# --install-deps-from installs the GNOME runtime, SDK and Rust extension for this user when
# they are missing. The manifest runs the shell's tests during the build, so a failing test
# fails the package.
flatpak-builder \
    --user \
    --arch="$arch" \
    --force-clean \
    --install-deps-from=flathub \
    --state-dir="$state" \
    --repo="$stage/repo" \
    "$stage/build" \
    "$manifest"

flatpak build-bundle \
    --arch="$arch" \
    --runtime-repo=https://dl.flathub.org/repo/flathub.flatpakrepo \
    "$stage/repo" \
    "$dist/$name.flatpak" \
    "$app_id"

(cd "$dist" && sha256sum "$name.flatpak" > SHA256SUMS-linux.txt)

echo "$dist/$name.flatpak"
cat "$dist/SHA256SUMS-linux.txt"
