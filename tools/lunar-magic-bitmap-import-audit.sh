#!/bin/sh
set -eu

usage() {
    echo "usage: tools/lunar-magic-bitmap-import-audit.sh OUTPUT_DIR BITMAP.bmp" >&2
    echo "  Run only against a disposable Lunar Magic 3.63 process with a ROM loaded." >&2
    echo "  The authenticated Editors -> 16x16 Tile Map command opens the modeless editor." >&2
    echo "  LM_WINE_EXECUTABLE: target process name (default: LMBitmapOracle.exe)" >&2
    echo "  LM_MINGW_CC: 32-bit MinGW compiler (default: i686-w64-mingw32-gcc)" >&2
    echo "  LM_BITMAP_REDUCTION: median-cut or popularity (default: median-cut)" >&2
    echo "  LM_BITMAP_PRIORITY: Popularity priority from 1 through 4 (default: 3)" >&2
    echo "  LM_BITMAP_MAX_COLORS: maximum reduced colors from 1 through 128 (default: 128)" >&2
    exit 2
}

[ "$#" -eq 2 ] || usage

output_dir=$1
bitmap=$2
workspace=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
target_executable=${LM_WINE_EXECUTABLE:-LMBitmapOracle.exe}
compiler=${LM_MINGW_CC:-i686-w64-mingw32-gcc}
reduction=${LM_BITMAP_REDUCTION:-median-cut}
priority=${LM_BITMAP_PRIORITY:-3}
maximum_colors=${LM_BITMAP_MAX_COLORS:-128}
helper="$output_dir/bin/wine-window-command.exe"
paste_log="$output_dir/paste.log"
paste_pid=
guard_set=0

case "$reduction" in
    median-cut|popularity) ;;
    *)
        echo "LM_BITMAP_REDUCTION must be median-cut or popularity" >&2
        exit 2
        ;;
esac
case "$priority" in
    1|2|3|4) ;;
    *)
        echo "LM_BITMAP_PRIORITY must be from 1 through 4" >&2
        exit 2
        ;;
esac
case "$maximum_colors" in
    *[!0-9]*|'')
        echo "LM_BITMAP_MAX_COLORS must be from 1 through 128" >&2
        exit 2
        ;;
esac
if [ "$maximum_colors" -lt 1 ] || [ "$maximum_colors" -gt 128 ]; then
    echo "LM_BITMAP_MAX_COLORS must be from 1 through 128" >&2
    exit 2
fi

[ -f "$bitmap" ] || {
    echo "bitmap does not exist: $bitmap" >&2
    exit 1
}
[ ! -e "$output_dir" ] || {
    echo "output path already exists: $output_dir" >&2
    exit 1
}
command -v wine >/dev/null 2>&1 || {
    echo "wine is required" >&2
    exit 1
}
command -v winepath >/dev/null 2>&1 || {
    echo "winepath is required" >&2
    exit 1
}
command -v "$compiler" >/dev/null 2>&1 || {
    echo "32-bit MinGW compiler is required: $compiler" >&2
    exit 1
}
command -v xxd >/dev/null 2>&1 || {
    echo "xxd is required" >&2
    exit 1
}

mkdir -p "$output_dir/bin"
"$compiler" -std=c11 -O2 -Wall -Wextra -Werror \
    "$workspace/tools/wine-window-command.c" -lcomctl32 -lgdi32 -o "$helper"

restore_guard() {
    if [ "$guard_set" -eq 1 ]; then
        wine "$helper" "$target_executable" write-byte 0x00e277cc,0 >/dev/null 2>&1 || true
        guard_set=0
    fi
}

cleanup() {
    restore_guard
    if [ -n "$paste_pid" ] && kill -0 "$paste_pid" 2>/dev/null; then
        kill "$paste_pid" 2>/dev/null || true
    fi
}
trap cleanup EXIT HUP INT TERM

read_value() {
    wine "$helper" "$target_executable" read "$1,$2" 2>/dev/null | tr -d '\r\n'
}

level_loaded=$(read_value 0x00e2782a 1)
[ "$level_loaded" = "01" ] || {
    echo "the target process does not have a ROM level loaded" >&2
    exit 1
}
current_level=$(read_value 0x005e7738 4)

# A modeless Map16 window restored before ROM loading retains stale palette/graphics buffers.
# Reload the authenticated current slot through Lunar Magic's own level dialog before observing
# those buffers. The first two little-endian bytes cover Lunar Magic's 0x000..0x1ff level range.
current_level_hex=$(printf '%s' "$current_level" | cut -c 3-4)$(printf '%s' "$current_level" | cut -c 1-2)
wine "$helper" "$target_executable" open-level "$current_level_hex" >/dev/null 2>&1
refreshed_level=$(read_value 0x005e7738 4)
[ "$refreshed_level" = "$current_level" ] || {
    echo "Lunar Magic did not reload the current level $current_level_hex" >&2
    exit 1
}

modeless_handle=$(read_value 0x00a09270 4)
if [ "$modeless_handle" = "00000000" ]; then
# HandleLevelEditorCommand routes 0x232f through RestoreOpenAuxiliaryEditorWindows and
# ShowMap16EditorDialog. Canonicalize a persisted stale open flag before issuing the command.
    wine "$helper" "$target_executable" write-byte 0x00e27828,0 >/dev/null 2>&1
    wine "$helper" "$target_executable" post-command 0x232f >/dev/null 2>&1
    attempt=0
    while [ "$attempt" -lt 200 ]; do
        modeless_handle=$(read_value 0x00a09270 4)
        [ "$modeless_handle" != "00000000" ] && break
        attempt=$((attempt + 1))
        sleep 0.025
    done
fi
[ "$modeless_handle" != "00000000" ] || {
    echo "the modeless 16x16 Tile Map Editor did not open within 5 seconds" >&2
    exit 1
}

read_buffer() {
    address=$1
    length=$2
    output=$3
    wine "$helper" "$target_executable" read "$address,$length" 2>/dev/null |
        xxd -r -p >"$output"
}

read_buffer 0x00758dd8 1024 "$output_dir/palette-before.rgb32"
read_buffer 0x0086b7e8 65536 "$output_dir/graphics-before.bin"

bitmap_windows=$(winepath -w "$bitmap")
wine "$helper" "$target_executable" write-byte 0x00e277cc,2 >/dev/null 2>&1
guard_set=1
wine "$helper" "$target_executable" clipboard-bmp-paste "$bitmap_windows" \
    >"$paste_log" 2>&1 &
paste_pid=$!

dialog_ready=0
attempt=0
while [ "$attempt" -lt 200 ]; do
    if wine "$helper" "$target_executable" list 2>/dev/null |
        grep -q 'title=Convert and Paste Bitmap (in hex)'; then
        dialog_ready=1
        break
    fi
    if ! kill -0 "$paste_pid" 2>/dev/null; then
        break
    fi
    attempt=$((attempt + 1))
    sleep 0.025
done
[ "$dialog_ready" -eq 1 ] || {
    echo "bitmap conversion dialog was not ready within 5 seconds" >&2
    exit 1
}

restore_guard
if [ "$reduction" = "popularity" ]; then
    wine "$helper" "$target_executable" click 0x6b >/dev/null 2>&1
    color_dialog_ready=0
    attempt=0
    while [ "$attempt" -lt 200 ]; do
        if wine "$helper" "$target_executable" list 2>/dev/null |
            grep -q 'title=Bitmap Pasting Color Options'; then
            color_dialog_ready=1
            break
        fi
        attempt=$((attempt + 1))
        sleep 0.025
    done
    [ "$color_dialog_ready" -eq 1 ] || {
        echo "bitmap color-options dialog was not ready within 5 seconds" >&2
        exit 1
    }
    wine "$helper" "$target_executable" select 0x69,1 >/dev/null 2>&1
    wine "$helper" "$target_executable" select "0x78,$((maximum_colors - 1))" >/dev/null 2>&1
    wine "$helper" "$target_executable" select "0x71,$((priority - 1))" >/dev/null 2>&1
    wine "$helper" "$target_executable" dialog-values \
        >"$output_dir/color-options.txt" 2>/dev/null
    wine "$helper" "$target_executable" click 1 >/dev/null 2>&1
fi
wine "$helper" "$target_executable" list '#32770' 2>/dev/null |
    sed -n '/title=Convert and Paste Bitmap (in hex)/,/title=16x16 Tile Map Editor/p' \
    >"$output_dir/dialog.txt"
wine "$helper" "$target_executable" click 1 >/dev/null 2>&1
wait "$paste_pid"
paste_pid=

read_buffer 0x00758dd8 1024 "$output_dir/palette-after.rgb32"
read_buffer 0x0086b7e8 65536 "$output_dir/graphics-after.bin"

palette_differences=$(cmp -l \
    "$output_dir/palette-before.rgb32" "$output_dir/palette-after.rgb32" | wc -l | tr -d ' ')
graphics_differences=$(cmp -l \
    "$output_dir/graphics-before.bin" "$output_dir/graphics-after.bin" | wc -l | tr -d ' ')
palette_before_sha=$(shasum -a 256 "$output_dir/palette-before.rgb32" | awk '{print $1}')
palette_after_sha=$(shasum -a 256 "$output_dir/palette-after.rgb32" | awk '{print $1}')
graphics_before_sha=$(shasum -a 256 "$output_dir/graphics-before.bin" | awk '{print $1}')
graphics_after_sha=$(shasum -a 256 "$output_dir/graphics-after.bin" | awk '{print $1}')

{
    printf 'field\tvalue\n'
    printf 'target_executable\t%s\n' "$target_executable"
    printf 'modeless_handle\t%s\n' "$modeless_handle"
    printf 'current_level_le32\t%s\n' "$current_level"
    printf 'reduction\t%s\n' "$reduction"
    printf 'priority\t%s\n' "$priority"
    printf 'maximum_colors\t%s\n' "$maximum_colors"
    printf 'palette_byte_differences\t%s\n' "$palette_differences"
    printf 'graphics_byte_differences\t%s\n' "$graphics_differences"
    printf 'palette_before_sha256\t%s\n' "$palette_before_sha"
    printf 'palette_after_sha256\t%s\n' "$palette_after_sha"
    printf 'graphics_before_sha256\t%s\n' "$graphics_before_sha"
    printf 'graphics_after_sha256\t%s\n' "$graphics_after_sha"
} >"$output_dir/manifest.tsv"

echo "bitmap import audit: $output_dir"
echo "palette byte differences: $palette_differences"
echo "graphics byte differences: $graphics_differences"
