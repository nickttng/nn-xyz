#!/usr/bin/env bash
# Privacy guard for kwaj. Blocks content that must never reach a public repo.
#
#   scripts/guard.sh --staged   check files staged for commit (pre-commit hook)
#   scripts/guard.sh --all      check every tracked file (CI)
#
# Rules:
#   1. Every Markdown file in content/ is explicitly published: `publish: true` or
#      `draft: false` in its front matter, and never `draft: true`.
#   2. No image anywhere in the repo carries EXIF/XMP/IPTC metadata (GPS, device, timestamps).
set -euo pipefail

mode="${1:---all}"
fail=0

case "$mode" in
  --staged) files=$(git diff --cached --name-only --diff-filter=ACMR) ;;
  --all)    files=$(git ls-files) ;;
  *) echo "usage: $0 [--staged|--all]" >&2; exit 2 ;;
esac

# Read the version git will actually commit, not the working copy.
read_file() {
  if [[ "$mode" == "--staged" ]]; then git show ":$1"; else cat "$1"; fi
}

# 1. publish: true on every note
while IFS= read -r f; do
  [[ "$f" == content/*.md ]] || continue
  front=$(read_file "$f" | awk 'NR==1 && $0!="---" {exit} NR>1 && $0=="---" {exit} NR>1 {print}')
  if grep -Eq '^draft:[[:space:]]*true[[:space:]]*$' <<<"$front"; then
    echo "BLOCK  $f  is 'draft: true' (drafts stay outside the repo)"
    fail=1
  elif ! grep -Eq '^(publish:[[:space:]]*true|draft:[[:space:]]*false)[[:space:]]*$' <<<"$front"; then
    echo "BLOCK  $f  needs 'publish: true' or 'draft: false' (private notes stay outside the repo)"
    fail=1
  fi
done <<<"$files"

# 2. No image metadata
images=$(grep -Ei '\.(jpe?g|png|heic|heif|tiff?|webp|avif|gif)$' <<<"$files" || true)
if [[ -n "$images" ]]; then
  if ! command -v exiftool >/dev/null; then
    echo "BLOCK  exiftool not installed; cannot verify image metadata (brew install exiftool)"
    exit 1
  fi
  tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
  while IFS= read -r img; do
    copy="$tmp/$(basename "$img")"
    read_file "$img" >"$copy"
    found=$(exiftool -q -q -s -EXIF:all -XMP:all -IPTC:all -GPS:all "$copy" | head -5)
    if [[ -n "$found" ]]; then
      echo "BLOCK  $img  has metadata; strip it with: exiftool -all= -overwrite_original \"$img\""
      sed 's/^/         /' <<<"$found"
      fail=1
    fi
  done <<<"$images"
fi

if [[ $fail -ne 0 ]]; then
  echo
  echo "Privacy guard failed. Nothing was committed."
  exit 1
fi
echo "Privacy guard: ok"
