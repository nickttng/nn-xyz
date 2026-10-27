#!/usr/bin/env bash
# Inspect and optimize the images in content/images/ for fast, sharp pages.
#
#   scripts/images.sh                 audit every image and every note (changes nothing)
#   scripts/images.sh optimize FILE…  resize and re-encode to WebP; never makes a file bigger
#
# Needs macOS `sips` (built in), plus `cwebp` and `exiftool`:
#   brew install webp exiftool
# Guidance lives in .agents/skills/web-images/SKILL.md.
set -euo pipefail

# Budgets. Change them here; the skill explains where they come from.
MAX_W=1600        # px. The page is 42rem (672px); 1600 covers 2x screens with room to spare.
MAX_H=2000        # px. Tall portraits stay sharp without shipping 4032px of height.
MAX_KB=250        # per image
TARGET_KB=150     # what a well-encoded 1600px photo usually lands under
PAGE_KB=1500      # all images on one note, together
QUALITY=80        # cwebp quality for photos

root="$(cd "$(dirname "$0")/.." && pwd)"
content="${KWAJ_CONTENT_DIR:-$root/content}"
images="$content/images"

kb() { echo $(( $(stat -f%z "$1") / 1024 )); }
dims() { sips -g pixelWidth -g pixelHeight "$1" 2>/dev/null | awk '/pixelWidth/{w=$2} /pixelHeight/{h=$2} END{print w+0, h+0}'; }
profile() { sips -g profile "$1" 2>/dev/null | awk -F': ' '/profile/{print $2}'; }
metadata() { exiftool -q -q -s -EXIF:all -XMP:all -IPTC:all -GPS:all "$1" | wc -l | tr -d ' '; }

audit() {
  [[ -d "$images" ]] || { echo "No $images folder yet."; exit 0; }
  local problems=0
  echo "== Images in ${images#"$root"/}"
  printf "%-28s %6s %11s  %s\n" "file" "KB" "size" "notes"
  for img in "$images"/*; do
    [[ -f "$img" ]] || continue
    local name flags="" w h p
    name="$(basename "$img")"
    read -r w h < <(dims "$img")
    case "${name##*.}" in
      webp|avif|svg) ;;
      jpg|jpeg|png|gif) flags+=" convert-to-webp" ;;
      *) flags+=" unsupported-format" ;;
    esac
    (( w > MAX_W || h > MAX_H )) && flags+=" oversized(>${MAX_W}x${MAX_H})"
    (( $(kb "$img") > MAX_KB )) && flags+=" heavy(>${MAX_KB}KB)"
    p="$(profile "$img")"
    [[ -n "$p" && "$p" != "<nil>" && "$p" != *sRGB* ]] && flags+=" colour-profile($p)"
    (( $(metadata "$img") > 0 )) && flags+=" has-metadata"
    grep -rqF "images/$name" "$content"/*.md || flags+=" unused"
    [[ -n "$flags" ]] && problems=$((problems + 1))
    printf "%-28s %6s %11s %s\n" "$name" "$(kb "$img")" "${w}x${h}" "${flags:- ok}"
  done

  echo
  echo "== Notes with images"
  for note in "$content"/*.md; do
    local refs total=0 count=0 missing="" placeholder
    refs="$(grep -oE '!\[[^]]*\]\(images/[^)]+\)' "$note" | sed -E 's/.*\(images\/([^)]+)\)/\1/' || true)"
    [[ -n "$refs" ]] || continue
    while read -r ref; do
      count=$((count + 1))
      if [[ -f "$images/$ref" ]]; then total=$((total + $(kb "$images/$ref"))); else missing+=" $ref"; fi
    done <<<"$refs"
    placeholder="$(grep -ciE '!\[(alt text|description|image)?\]\(' "$note" || true)"
    local flags=""
    (( total > PAGE_KB )) && flags+=" over-page-budget(>${PAGE_KB}KB)"
    [[ -n "$missing" ]] && flags+=" missing:$missing"
    (( placeholder > 0 )) && flags+=" placeholder-alt($placeholder)"
    [[ -n "$flags" ]] && problems=$((problems + 1))
    printf "%-32s %3s images %6s KB %s\n" "$(basename "$note")" "$count" "$total" "${flags:- ok}"
  done

  echo
  if (( problems == 0 )); then echo "All images within budget."; else echo "$problems item(s) to look at. See .agents/skills/web-images/SKILL.md."; fi
}

optimize() {
  command -v cwebp >/dev/null || { echo "cwebp not installed: brew install webp"; exit 1; }
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT
  for src in "$@"; do
    [[ -f "$src" ]] || { echo "skip   $src (not found)"; continue; }
    local name base out w h nw mode
    name="$(basename "$src")"
    base="${name%.*}"
    out="$(dirname "$src")/$base.webp"
    case "${name##*.}" in
      svg|gif|avif) echo "skip   $name (${name##*.} is left as-is)"; continue ;;
      png) mode=(-lossless -z 9) ;;   # screenshots and diagrams: keep every pixel
      *) mode=(-q "$QUALITY" -m 6 -sharp_yuv) ;;
    esac

    # Convert to sRGB first, so stripping metadata later can't dull the colours.
    sips -s format png --matchTo "/System/Library/ColorSync/Profiles/sRGB Profile.icc" "$src" --out "$tmp/$base.png" >/dev/null
    read -r w h < <(dims "$tmp/$base.png")
    nw="$w"
    (( nw > MAX_W )) && nw="$MAX_W"
    (( h * nw / w > MAX_H )) && nw=$(( w * MAX_H / h ))

    cwebp -quiet "${mode[@]}" -metadata none -resize "$nw" 0 "$tmp/$base.png" -o "$tmp/$base.webp"

    local before after
    before="$(kb "$src")"
    after="$(kb "$tmp/$base.webp")"
    # Already well compressed: re-encoding would only add weight (and lose a little detail).
    if (( after >= before )); then
      echo "keep   $name ($before KB; re-encoding would make it $after KB)"
      continue
    fi
    mv "$tmp/$base.webp" "$out"
    echo "wrote  $(basename "$out")  ${w}x${h} -> ${nw}px wide, $before KB -> $after KB"
    if [[ "$src" != "$out" ]]; then
      echo "       update links from images/$name to images/$base.webp, then delete $name"
    fi
  done
}

case "${1:-audit}" in
  audit) audit ;;
  optimize) shift; (( $# )) || { echo "usage: scripts/images.sh optimize FILE…"; exit 1; }; optimize "$@" ;;
  *) echo "usage: scripts/images.sh [audit | optimize FILE…]"; exit 1 ;;
esac
