#!/bin/sh
set -eu

usage() {
    echo "usage: tools/render-audit.sh OUTPUT_DIR ROM [LEVELS] [SCREENS] [STYLES]" >&2
    echo "  LEVELS: comma-separated hexadecimal slots or 'all' (default: all)" >&2
    echo "  SCREENS: comma-separated hexadecimal major-axis screens (default: 0)" >&2
    echo "  STYLES: comma-separated 'game' and/or 'editor' (default: game)" >&2
    exit 2
}

[ "$#" -ge 2 ] && [ "$#" -le 5 ] || usage

output_dir=$1
rom=$2
level_spec=${3:-all}
screen_spec=${4:-0}
style_spec=${5:-game}
workspace=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
binary="$workspace/target/debug/lm-native"

[ -f "$rom" ] || {
    echo "ROM does not exist: $rom" >&2
    exit 1
}

mkdir -p "$output_dir/images"

(
    cd "$workspace"
    cargo build -p lm-native --features visual-smoke
)

if command -v shasum >/dev/null 2>&1; then
    rom_sha1=$(shasum "$rom" | awk '{print $1}')
else
    rom_sha1=$(sha1sum "$rom" | awk '{print $1}')
fi

if [ "$level_spec" = all ]; then
    levels=$(awk 'BEGIN { for (i = 0; i < 512; ++i) printf "%03X%s", i, i == 511 ? ORS : "," }')
else
    levels=$level_spec
fi

manifest="$output_dir/manifest.tsv"
html="$output_dir/index.html"
printf 'level\tstyle\tscreen\tmajor_tiles\tsha256\timage\n' >"$manifest"

old_ifs=$IFS
IFS=,
for level in $levels; do
    for style in $style_spec; do
        case "$style" in
            game|editor) ;;
            *) echo "unknown render style: $style" >&2; exit 2 ;;
        esac
        for screen in $screen_spec; do
            major_tiles=$((0x$screen * 16))
            image_name="level-${level}-${style}-screen-${screen}.png"
            image="$output_dir/images/$image_name"
            if [ ! -f "$image" ]; then
                LM_NATIVE_SCREENSHOT_TO="$image" \
                LM_NATIVE_PREVIEW_STYLE="$style" \
                LM_NATIVE_PREVIEW_CAMERA_MAJOR="$major_tiles" \
                    "$binary" --level "$level" "$rom"
            fi
            if command -v shasum >/dev/null 2>&1; then
                digest=$(shasum -a 256 "$image" | awk '{print $1}')
            else
                digest=$(sha256sum "$image" | awk '{print $1}')
            fi
            printf '%s\t%s\t%s\t%s\t%s\timages/%s\n' \
                "$level" "$style" "$screen" "$major_tiles" "$digest" "$image_name" >>"$manifest"
        done
    done
done
IFS=$old_ifs

{
    printf '%s\n' '<!doctype html><meta charset="utf-8">'
    printf '%s\n' '<title>Lunar Magic Rust render audit</title>'
    printf '%s\n' '<style>body{font:14px system-ui;background:#171717;color:#eee;margin:20px}.meta{position:sticky;top:0;background:#171717;padding:8px 0}.grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(360px,1fr));gap:14px}.card{background:#262626;padding:10px;border-radius:8px}.card img{width:100%;height:auto;image-rendering:pixelated}.id{font:600 16px ui-monospace,monospace}.hash{font:11px ui-monospace,monospace;color:#aaa;overflow-wrap:anywhere}</style>'
    printf '<div class="meta">ROM SHA-1: <code>%s</code> · commit: <code>%s</code></div>\n' \
        "$rom_sha1" "$(git -C "$workspace" rev-parse --short HEAD)"
    printf '%s\n' '<div class="grid">'
    awk -F '\t' 'NR > 1 {
        printf "<article class=\"card\"><div class=\"id\">Level $%s · %s · screen $%s · major tile %s</div><a href=\"%s\"><img loading=\"lazy\" src=\"%s\"></a><div class=\"hash\">%s</div></article>\n", $1, $2, $3, $4, $6, $6, $5
    }' "$manifest"
    printf '%s\n' '</div>'
} >"$html"

echo "render audit: $html"
echo "manifest: $manifest"
