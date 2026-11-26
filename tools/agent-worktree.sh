#!/bin/sh
# Gives one assistant its own checkout of this repository, and puts a branch in it.
#
#   sh tools/agent-worktree.sh <agent> <branch> [<base>]
#
# <agent> names the assistant (claude, codex, gemini, or any other lowercase name). Each one
# keeps a single worktree beside the main checkout, at ../GoosicReborn-<agent>, and reuses it
# from task to task, so its Rust `target/` and SwiftPM `.build/` stay warm and are never the
# ones another assistant is building into.
#
# <branch> is checked out in that worktree. An existing local or remote branch is used as it
# is; a new one is cut from origin/<base>, `development` unless another base is named.
#
# The script refuses rather than guesses: it will not switch a worktree with uncommitted work,
# will not check out `main`, and will not take a branch another worktree already has. Git
# itself enforces that last rule, which is what stops two assistants committing on one branch.
# See docs/PARALLEL_AGENTS.md.
set -eu

usage() {
    echo "Usage: sh tools/agent-worktree.sh <agent> <branch> [<base>]" >&2
    exit 2
}

[ $# -ge 2 ] && [ $# -le 3 ] || usage
agent=$1
branch=$2
base=${3:-development}

case "$agent" in
    *[!a-z0-9-]* | "" | -*) echo "The agent name is lowercase letters, digits and dashes." >&2; exit 2 ;;
esac
case "$branch" in
    main) echo "main is deployment only; cut a hotfix/<slug> instead." >&2; exit 2 ;;
esac
git check-ref-format --branch "$branch" >/dev/null 2>&1 \
    || { echo "\"$branch\" is not a valid branch name." >&2; exit 2; }

# The main checkout is the parent of the shared .git directory, wherever this is run from.
common=$(git rev-parse --path-format=absolute --git-common-dir)
main_checkout=$(dirname "$common")
dir="$(dirname "$main_checkout")/$(basename "$main_checkout")-$agent"

git -C "$main_checkout" fetch --quiet --prune origin

# A branch checked out in some other worktree belongs to whoever is working there.
holder=$(git -C "$main_checkout" worktree list --porcelain | awk -v ref="refs/heads/$branch" '
    /^worktree / { path = substr($0, 10) }
    $0 == "branch " ref { print path }')
if [ -n "$holder" ] && [ "$holder" != "$dir" ]; then
    echo "$branch is checked out in $holder. Work there, or pick another branch." >&2
    exit 1
fi

has_local() { git -C "$main_checkout" rev-parse --verify --quiet "refs/heads/$branch" >/dev/null; }
has_remote() { git -C "$main_checkout" rev-parse --verify --quiet "refs/remotes/origin/$branch" >/dev/null; }

if [ ! -e "$dir" ]; then
    if has_local; then
        git -C "$main_checkout" worktree add --quiet "$dir" "$branch"
    elif has_remote; then
        git -C "$main_checkout" worktree add --quiet --track -b "$branch" "$dir" "origin/$branch"
    else
        git -C "$main_checkout" rev-parse --verify --quiet "refs/remotes/origin/$base" >/dev/null \
            || { echo "origin/$base does not exist." >&2; exit 1; }
        git -C "$main_checkout" worktree add --quiet --no-track -b "$branch" "$dir" "origin/$base"
    fi
else
    git -C "$dir" rev-parse --is-inside-work-tree >/dev/null 2>&1 \
        || { echo "$dir exists but is not a worktree of this repository." >&2; exit 1; }
    if [ -n "$(git -C "$dir" status --porcelain)" ]; then
        echo "$dir has uncommitted work on $(git -C "$dir" branch --show-current)." >&2
        echo "Commit or stash it there first; nothing was switched." >&2
        exit 1
    fi
    if has_local; then
        git -C "$dir" switch --quiet "$branch"
    elif has_remote; then
        git -C "$dir" switch --quiet --track -c "$branch" "origin/$branch"
    else
        git -C "$main_checkout" rev-parse --verify --quiet "refs/remotes/origin/$base" >/dev/null \
            || { echo "origin/$base does not exist." >&2; exit 1; }
        git -C "$dir" switch --quiet --no-track -c "$branch" "origin/$base"
    fi
fi

echo "$dir"
