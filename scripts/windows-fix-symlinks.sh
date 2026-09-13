#!/usr/bin/env bash
# Windows cannot create symlinks without Developer Mode, so the build checks
# dependencies out with core.symlinks=false. That leaves each symlink as a plain
# file holding its target path. This restores them as real copies.
#
# Targets are read from git rather than from the working tree, so an entry that
# has already been replaced by a copy can still be identified and redone. That
# matters because a symlinked directory may itself contain a symlink: the first
# pass copies it while its own link is still broken, and a later pass has to
# copy it again now that the source is whole. Passes run until nothing changes.
set -eu
checkouts="${1:-$LOCALAPPDATA/goosic-swift-build/checkouts}"

for _pass in 1 2 3; do
  changed=0
  for repo in "$checkouts"/*/; do
    [ -d "$repo/.git" ] || continue
    while IFS=$'\t' read -r meta entry; do
      case "$meta" in 120000*) ;; *) continue ;; esac
      sha=$(printf '%s' "$meta" | awk '{print $2}')
      target=$(git -C "$repo" cat-file blob "$sha")
      source="$repo/$(dirname "$entry")/$target"
      [ -e "$source" ] || continue
      # Nothing to do once the copy already matches the source.
      if [ -e "$repo/$entry" ] && diff -rq "$source" "$repo/$entry" >/dev/null 2>&1; then
        continue
      fi
      rm -rf "${repo:?}/$entry"
      cp -r "$source" "$repo/$entry"
      echo "restored ${repo#"$checkouts"/}$entry"
      changed=1
    done < <(git -C "$repo" ls-files -s)
  done
  [ "$changed" -eq 1 ] || break
  repaired=1
done

# SwiftPM records each target's source list, so a plugin scanned while its shared
# sources were still placeholders stays broken until the cache is dropped -- it
# reports the shared symbols as not in scope.
if [ "${repaired:-0}" -eq 1 ]; then
  scratch=$(dirname "$checkouts")
  rm -rf "$scratch/plugins" "$scratch/build.db" "$scratch/plugin-tools.yaml" "$scratch/debug.yaml"
  echo "cleared SwiftPM caches under $scratch"
fi
