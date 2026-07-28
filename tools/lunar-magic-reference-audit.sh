#!/bin/sh
set -eu

usage() {
    echo "usage: tools/lunar-magic-reference-audit.sh OUTPUT_DIR LEVELS [RUST_AUDIT_DIR]" >&2
    echo "  LEVELS: comma-separated hexadecimal Lunar Magic level slots" >&2
    echo "  RUST_AUDIT_DIR: optional render-audit directory for side-by-side editor images" >&2
    echo "  LM_WINE_EXECUTABLE: target process name (default: Lunar Magic.exe)" >&2
    exit 2
}

[ "$#" -ge 2 ] && [ "$#" -le 3 ] || usage

output_dir=$1
level_spec=$2
rust_audit_dir=${3:-}
workspace=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
target_executable=${LM_WINE_EXECUTABLE:-Lunar Magic.exe}
compiler=${LM_MINGW_CC:-i686-w64-mingw32-gcc}
window_helper="$output_dir/bin/wine-window-command.exe"
capture_helper="$output_dir/bin/wine-level-dib-capture.exe"
images_dir="$output_dir/images"
manifest="$output_dir/manifest.tsv"
html="$output_dir/index.html"

command -v wine >/dev/null 2>&1 || {
    echo "wine is required" >&2
    exit 1
}
command -v "$compiler" >/dev/null 2>&1 || {
    echo "32-bit MinGW compiler is required: $compiler" >&2
    exit 1
}
command -v sips >/dev/null 2>&1 || {
    echo "sips is required to convert the captured BMP files to PNG" >&2
    exit 1
}
[ -z "$rust_audit_dir" ] || [ -f "$rust_audit_dir/manifest.tsv" ] || {
    echo "Rust audit manifest does not exist: $rust_audit_dir/manifest.tsv" >&2
    exit 1
}

mkdir -p "$output_dir/bin" "$images_dir"
"$compiler" -std=c11 -O2 -Wall -Wextra -Werror \
    "$workspace/tools/wine-window-command.c" -lcomctl32 -o "$window_helper"
"$compiler" -std=c11 -O2 -Wall -Wextra -Werror \
    "$workspace/tools/wine-level-dib-capture.c" -o "$capture_helper"

read_u32() {
    wine "$window_helper" "$target_executable" read "$1,4" 2>/dev/null |
        xxd -r -p |
        od -An -tu4 |
        tr -d ' '
}

wait_for_level() {
    expected=$((0x$1))
    attempts=0
    while [ "$attempts" -lt 250 ]; do
        current=$(read_u32 0x005e7738)
        [ "$current" -eq "$expected" ] && return 0
        attempts=$((attempts + 1))
        sleep 0.02
    done
    echo "Lunar Magic did not select level $1 within 5 seconds" >&2
    return 1
}

capture_stable_dib() {
    output=$1
    candidate="$output.next"
    wine "$capture_helper" "$target_executable" "$(winepath -w "$output")" >/dev/null 2>&1
    attempts=0
    while [ "$attempts" -lt 12 ]; do
        sleep 0.06
        wine "$capture_helper" "$target_executable" "$(winepath -w "$candidate")" >/dev/null 2>&1
        if cmp -s "$output" "$candidate"; then
            rm -f "$candidate"
            return 0
        fi
        mv "$candidate" "$output"
        attempts=$((attempts + 1))
    done
    rm -f "$candidate"
    echo "accept Lunar Magic level $normalized after animation settle" >&2
    return 0
}

printf 'level\tsha256\tlunar_magic_image\trust_editor_image\tscroll_column\tscroll_row\tcanvas_width\tcanvas_height\n' >"$manifest"
old_ifs=$IFS
IFS=,
for level in $level_spec; do
    case "$level" in
        ''|*[!0-9A-Fa-f]*)
            echo "invalid hexadecimal level slot: $level" >&2
            exit 2
            ;;
    esac
    normalized=$(printf '%03X' "$((0x$level))")
    bmp="$images_dir/lunar-magic-level-$normalized.bmp"
    png="$images_dir/lunar-magic-level-$normalized.png"
    echo "open Lunar Magic level $normalized"
    wine "$window_helper" "$target_executable" open-level "$normalized" >/dev/null
    wait_for_level "$normalized"
    # The selected-level global changes before Lunar Magic finishes rebuilding and
    # compositing every layer.  An unchanged early DIB can therefore be a stable
    # intermediate frame (notably the Layer 2-only frame of level 118).
    sleep 0.50
    echo "capture stable Lunar Magic level $normalized"
    capture_stable_dib "$bmp"
    sips -s format png "$bmp" --out "$png" >/dev/null
    rm -f "$bmp"
    digest=$(shasum -a 256 "$png" | awk '{print $1}')
    scroll_column=$(read_u32 0x006204a4)
    scroll_row=$(read_u32 0x009207dc)
    canvas_width=$(read_u32 0x008636dc)
    canvas_height=$(read_u32 0x007308c4)
    rust_image=
    if [ -n "$rust_audit_dir" ]; then
        candidate="$rust_audit_dir/images/level-$normalized-editor-screen-0.png"
        if [ -f "$candidate" ]; then
            rust_image=$candidate
        fi
    fi
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$normalized" "$digest" "$png" "$rust_image" \
        "$scroll_column" "$scroll_row" "$canvas_width" "$canvas_height" >>"$manifest"
done
IFS=$old_ifs

{
    printf '%s\n' '<!doctype html><meta charset="utf-8">'
    printf '%s\n' '<title>Lunar Magic live-reference audit</title>'
    printf '%s\n' '<style>body{font:14px system-ui;background:#171717;color:#eee;margin:20px}.grid{display:grid;gap:18px}.card{background:#262626;padding:12px;border-radius:8px}.pair{display:grid;grid-template-columns:repeat(auto-fit,minmax(420px,1fr));gap:12px}.pair img{width:100%;height:auto;image-rendering:pixelated}.id{font:600 17px ui-monospace,monospace}.label{margin:8px 0 4px}.hash{font:11px ui-monospace,monospace;color:#aaa;overflow-wrap:anywhere}</style>'
    printf '<div class="grid">\n'
    awk -F '\t' 'NR > 1 {
        printf "<article class=\"card\"><div class=\"id\">Level $%s</div><div class=\"pair\">", $1
        printf "<div><div class=\"label\">Lunar Magic live DIB</div><a href=\"%s\"><img loading=\"lazy\" src=\"%s\"></a></div>", $3, $3
        if ($4 != "") {
            printf "<div><div class=\"label\">Rust editor framebuffer</div><a href=\"%s\"><img loading=\"lazy\" src=\"%s\"></a></div>", $4, $4
        }
        printf "</div><div class=\"hash\">Lunar Magic SHA-256: %s</div></article>\n", $2
    }' "$manifest"
    printf '%s\n' '</div>'
} >"$html"

echo "live-reference audit: $html"
echo "manifest: $manifest"
