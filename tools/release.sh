#!/bin/sh
# Runs a release from the trunk to a draft, and the Mac update feed after it is published.
#
#   sh tools/release.sh <x.y.z>        freeze the version, promote the platform branches, open
#                                      the deploy pull request and ask Codex to review it
#   sh tools/release.sh ship <x.y.z>   merge the deploy pull request once CI is green; the
#                                      release workflow then tags it and builds the draft
#   sh tools/release.sh feed <x.y.z>   after the draft is published: sign the Mac ZIP and add it
#                                      to updates/macos-alpha.xml on development
#
# Every step can be run again: it finds what an earlier run already did and carries on from
# there, so an interrupted release is resumed by repeating the same command. It never touches
# the checkout it is run from; the work happens in a temporary worktree. See docs/RELEASING.md.
#
# The branches have no required checks, so `gh pr merge --auto` would merge at once instead of
# waiting. This script waits for every check itself and merges only when they have all passed.
set -eu

usage() {
    echo "Usage: sh tools/release.sh [ship|feed] <x.y.z>" >&2
    exit 2
}

case "${1:-}" in
    ship | feed) step=$1; version=${2:-} ;;
    *) step=prepare; version=${1:-} ;;
esac
printf '%s\n' "$version" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$' || usage
tag="v$version"
platforms="macos windows"

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
command -v gh >/dev/null || { echo "The GitHub CLI (gh) is required." >&2; exit 1; }
gh auth status >/dev/null 2>&1 || { echo "Sign in first: gh auth login" >&2; exit 1; }
git fetch --quiet origin

say() { printf '\n==> %s\n' "$*"; }

# The open pull request from $1 into $2, or nothing.
open_pr() {
    gh pr list --state open --head "$1" --base "$2" --json number --jq '.[0].number // empty'
}

# Opens a pull request from $1 into $2 unless one is open, and prints its number.
ensure_pr() {
    head=$1 base=$2 title=$3 body=$4
    number=$(open_pr "$head" "$base")
    if [ -z "$number" ]; then
        gh pr create --head "$head" --base "$base" --title "$title" --body "$body" >/dev/null
        number=$(open_pr "$head" "$base")
    fi
    echo "$number"
}

# Waits until every check on pull request $1 has finished, fails if one failed, then merges it
# with a merge commit, which is what the branch history is built from.
merge_when_green() {
    number=$1
    say "Waiting for CI on #$number"
    empty=0
    passed=0
    # gh exits 0 when every check passed, 8 while any is pending, and 1 both when one failed and
    # when none is reported. None is reported for a moment after a push, and again whenever the
    # base moves and the checks restart, which is what happens to the second pull request into
    # development once the first merges. So "none" is waited out, never read as a failure, and a
    # pass is believed only when it is still a pass on the next look.
    while :; do
        code=0
        output=$(gh pr checks "$number" 2>&1) || code=$?
        case $code in
            0)
                passed=$((passed + 1))
                [ "$passed" -lt 2 ] || break
                sleep 20
                ;;
            8)
                passed=0
                sleep 30
                ;;
            *)
                passed=0
                if printf '%s' "$output" | grep -q "no checks reported"; then
                    empty=$((empty + 1))
                    [ "$empty" -lt 40 ] || { echo "No checks reported on #$number." >&2; exit 1; }
                    sleep 15
                else
                    printf '%s\n' "$output" | grep -v -e '	pass	' -e '	skipping	' >&2 || true
                    echo "CI failed on #$number: $(gh pr view "$number" --json url --jq .url)" >&2
                    exit 1
                fi
                ;;
        esac
    done
    gh pr merge "$number" --merge >/dev/null
    echo "Merged #$number."
}

workspace_version() {
    git show "$1:Cargo.toml" | sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\([^"]*\)"/\1/p'
}

worktree=""
cleanup() { [ -z "$worktree" ] || git worktree remove --force "$worktree" >/dev/null 2>&1 || true; }
trap cleanup EXIT INT TERM

# A worktree on a new branch $1 cut from development.
work_on() {
    worktree=$(mktemp -d "${TMPDIR:-/tmp}/goosic-release.XXXXXX")
    rmdir "$worktree"
    if git rev-parse --verify --quiet "origin/$1" >/dev/null; then
        git worktree add --quiet -B "$1" "$worktree" "origin/$1"
    else
        git worktree add --quiet -B "$1" "$worktree" origin/development
    fi
}

prepare() {
    notes="docs/RELEASE_$version.md"
    pending=""

    # 1. Freeze the version on development, with its notes.
    if [ "$(workspace_version origin/development)" = "$version" ]; then
        echo "development is already at $version."
    else
        if ! git cat-file -e "origin/development:$notes" 2>/dev/null && [ ! -f "$notes" ]; then
            echo "Write $notes first; the release publishes it as the notes." >&2
            exit 1
        fi
        branch="release/$version"
        work_on "$branch"
        [ -f "$worktree/$notes" ] || cp "$notes" "$worktree/$notes"
        if [ "$(workspace_version "$branch")" != "$version" ]; then
            (cd "$worktree" && sh tools/bump-version.sh "$version" >/dev/null)
        fi
        (
            cd "$worktree"
            git add -A
            git diff --cached --quiet || git commit --quiet -m "Freeze the version at $version

Moves every version default a release reads to $version and adds its notes, before the
deploy pull request opens, so the deployment carries the final number."
            git push --quiet -u origin "$branch"
        )
        cleanup
        worktree=""
        pending="$pending $(ensure_pr "$branch" development "Freeze the version at $version" \
            "Moves every version default to $version and adds \`$notes\`. Opened by \`tools/release.sh\`.")"
    fi

    # 2. Promote each platform branch that holds work development does not have.
    for os in $platforms; do
        if [ -n "$(git rev-list --max-count=1 "origin/development..origin/platform/$os")" ]; then
            pending="$pending $(ensure_pr "platform/$os" development "Promote platform/$os for $version" \
                "Brings platform/$os into development for $version. Opened by \`tools/release.sh\`.")"
        else
            echo "platform/$os has nothing development lacks."
        fi
    done

    for number in $pending; do merge_when_green "$number"; done
    git fetch --quiet origin

    # 3. The deploy pull request, reviewed once by Codex: its diff is everything since the
    #    last release.
    [ "$(workspace_version origin/development)" = "$version" ] \
        || { echo "development is not at $version yet." >&2; exit 1; }
    deploy=$(ensure_pr development main "Deploy $version" \
        "Deploys $version. Notes: \`docs/RELEASE_$version.md\`. Opened by \`tools/release.sh\`; merge with \`sh tools/release.sh ship $version\` once the Codex review is in.")
    if ! gh pr view "$deploy" --json comments --jq '.comments[].body' | grep -q '^@codex review'; then
        gh pr comment "$deploy" --body "@codex review" >/dev/null
    fi
    say "Deploy pull request: $(gh pr view "$deploy" --json url --jq .url)"
    echo "Codex has been asked to review it. When its findings are dealt with, run:"
    echo "    sh tools/release.sh ship $version"
}

ship() {
    deploy=$(open_pr development main)
    [ -n "$deploy" ] || { echo "No open deploy pull request from development into main." >&2; exit 1; }
    [ "$(workspace_version origin/development)" = "$version" ] \
        || { echo "development is not at $version." >&2; exit 1; }
    merge_when_green "$deploy"
    say "Deployed. The release workflow tags $tag and builds the draft."
    echo "Follow it with: gh run list --workflow Release --limit 1"
    echo "Inspect the downloads and publish the draft, then run:"
    echo "    sh tools/release.sh feed $version"
}

feed() {
    # A repeat after the feed item merged has nothing left to do; the merged pull request is no
    # longer open, so looking for one would try to open another.
    if git show origin/development:updates/macos-alpha.xml \
        | grep -q "<sparkle:shortVersionString>$version</sparkle:shortVersionString>"; then
        say "development's feed already offers $version to installed Macs."
        return 0
    fi
    [ "$(gh release view "$tag" --json isDraft --jq .isDraft)" = false ] \
        || { echo "Publish the $tag draft first; the feed points at its download." >&2; exit 1; }
    sign=${SPARKLE_SIGN_UPDATE:-}
    for candidate in "$HOME/Library/Caches/goosic-swift-build/artifacts/sparkle/Sparkle/bin/sign_update" \
        "$root/apps/goosic-swift/.build/artifacts/sparkle/Sparkle/bin/sign_update"; do
        [ -n "$sign" ] || [ ! -x "$candidate" ] || sign=$candidate
    done
    [ -n "$sign" ] || { echo "sign_update not found: run make build-swift, or set SPARKLE_SIGN_UPDATE." >&2; exit 1; }

    # An experimental release reaches only Macs that chose the experimental channel: Sparkle
    # skips an item whose channel the copy has not allowed, and older copies allow none.
    channel_line=""
    label="macOS alpha"
    if [ "$(gh release view "$tag" --json isPrerelease --jq .isPrerelease)" = true ]; then
        channel_line="            <sparkle:channel>experimental</sparkle:channel>"
        label="macOS experimental"
    fi

    zip="Goosic-$version-macos-universal.zip"
    scratch=$(mktemp -d "${TMPDIR:-/tmp}/goosic-feed.XXXXXX")
    gh release download "$tag" --pattern "$zip" --dir "$scratch"
    (cd "$scratch" && unzip -q "$zip" Goosic.app/Contents/Info.plist)
    build=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleVersion' "$scratch/Goosic.app/Contents/Info.plist")
    # Prints sparkle:edSignature="…" length="…", signed with the key in this Mac's keychain.
    signature=$("$sign" "$scratch/$zip")
    rm -rf "$scratch"

    branch="release/sparkle-$version"
    work_on "$branch"
    feedfile="$worktree/updates/macos-alpha.xml"
    if grep -q "<sparkle:shortVersionString>$version</sparkle:shortVersionString>" "$feedfile"; then
        echo "The feed already offers $version."
    else
        newest=$(sed -n 's:.*<sparkle\:version>\([0-9]*\)</sparkle\:version>.*:\1:p' "$feedfile" | sort -n | tail -1)
        [ "$build" -gt "${newest:-0}" ] \
            || { echo "Build $build is not above the feed's newest, $newest; Sparkle would offer nothing." >&2; exit 1; }
        url="https://github.com/AnalogicGoose/GoosicReborn/releases"
        item=$(mktemp)
        cat > "$item" <<EOF
        <item>
            <title>$version $label</title>
            <pubDate>$(LC_ALL=C date -u '+%a, %d %b %Y %H:%M:%S +0000')</pubDate>
            <sparkle:version>$build</sparkle:version>
            <sparkle:shortVersionString>$version</sparkle:shortVersionString>
            <sparkle:minimumSystemVersion>14.0</sparkle:minimumSystemVersion>
            <sparkle:releaseNotesLink>$url/tag/$tag</sparkle:releaseNotesLink>
$channel_line
            <enclosure url="$url/download/$tag/$zip" $signature type="application/octet-stream" />
        </item>
EOF
        # The newest item goes first, straight after the channel's title.
        awk -v item="$item" '{ print } /<title>Goosic<\/title>/ && !done { while ((getline line < item) > 0) print line; done = 1 }' \
            "$feedfile" > "$feedfile.new"
        mv "$feedfile.new" "$feedfile"
        rm -f "$item"
        (
            cd "$worktree"
            git commit --quiet -am "Offer $version to installed Macs through the Sparkle feed"
            git push --quiet -u origin "$branch"
        )
    fi
    cleanup
    worktree=""
    number=$(open_pr "$branch" development)
    [ -n "$number" ] || number=$(ensure_pr "$branch" development "Offer $version to installed Macs through the Sparkle feed" \
        "Adds build $build of $version to \`updates/macos-alpha.xml\`. Opened by \`tools/release.sh feed\`.")
    merge_when_green "$number"
    say "Installed Macs are now offered $version."
}

"$step"
