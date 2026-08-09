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
    echo "  LM_BITMAP_UNIQUE_COLORS: give higher priority to unique colors, 0 or 1 (default: 1)" >&2
    echo "  LM_BITMAP_MAINTAIN_DETAIL: keep exact bitmap colors until capacity, 0 or 1 (default: 0)" >&2
    echo "  LM_BITMAP_REDUCTION_METHOD_1: first Popularity neighborhood pass, 0 or 1 (default: 1)" >&2
    echo "  LM_BITMAP_REDUCTION_METHOD_2: second Popularity neighborhood pass, 0 or 1 (default: 0)" >&2
    echo "  LM_BITMAP_ALLOW_UNMARKED: allow changing unreserved palette colors, 0 or 1 (default: 1)" >&2
    echo "  LM_BITMAP_EXACT_MATCHES: disabled native exact-match state; only 1 is accepted" >&2
    echo "  LM_BITMAP_OPTIMIZE_8X8: optimize newly converted 8x8 tiles, 0 or 1 (default: 1)" >&2
    echo "  LM_BITMAP_REUSE_EXISTING_8X8: optimize with existing 8x8 tiles, 0 or 1 (default: 1)" >&2
    echo "  LM_BITMAP_OPTIMIZE_16X16: deduplicate 16x16 tiles, 0 or 1 (default: 1)" >&2
    echo "  LM_BITMAP_LAYER_PRIORITY: enable imported-tile priority, 0 or 1 (default: 0)" >&2
    echo "  LM_BITMAP_USE_BLANK_8X8: use configured blank 8x8 tile, 0 or 1 (default: 1)" >&2
    echo "  LM_BITMAP_USE_BLANK_16X16: use configured blank 16x16 tile, 0 or 1 (default: 1)" >&2
    echo "  LM_BITMAP_FIRST_8X8_TILE: hexadecimal first graphics tile (default: 200)" >&2
    echo "  LM_BITMAP_BLANK_8X8_TILE: hexadecimal blank graphics tile (default: 0F8)" >&2
    echo "  LM_BITMAP_FIRST_MAP16_TILE: hexadecimal first Map16 tile (default: 8200)" >&2
    echo "  LM_BITMAP_BLANK_MAP16_TILE: hexadecimal reserved blank Map16 tile (default: 8000)" >&2
    echo "  LM_BITMAP_EXPECT_MAP16_EXHAUSTION: capture and dismiss partial-import warning, 0 or 1 (default: 0)" >&2
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
unique_colors=${LM_BITMAP_UNIQUE_COLORS:-1}
maintain_detail=${LM_BITMAP_MAINTAIN_DETAIL:-0}
reduction_method_1=${LM_BITMAP_REDUCTION_METHOD_1:-1}
reduction_method_2=${LM_BITMAP_REDUCTION_METHOD_2:-0}
allow_unmarked=${LM_BITMAP_ALLOW_UNMARKED:-1}
exact_matches=${LM_BITMAP_EXACT_MATCHES:-1}
optimize_8x8=${LM_BITMAP_OPTIMIZE_8X8:-1}
reuse_existing_8x8=${LM_BITMAP_REUSE_EXISTING_8X8:-1}
optimize_16x16=${LM_BITMAP_OPTIMIZE_16X16:-1}
layer_priority=${LM_BITMAP_LAYER_PRIORITY:-0}
use_blank_8x8=${LM_BITMAP_USE_BLANK_8X8:-1}
use_blank_16x16=${LM_BITMAP_USE_BLANK_16X16:-1}
first_8x8_tile=${LM_BITMAP_FIRST_8X8_TILE:-200}
blank_8x8_tile=${LM_BITMAP_BLANK_8X8_TILE:-0F8}
first_map16_tile=${LM_BITMAP_FIRST_MAP16_TILE:-8200}
blank_map16_tile=${LM_BITMAP_BLANK_MAP16_TILE:-8000}
expect_map16_exhaustion=${LM_BITMAP_EXPECT_MAP16_EXHAUSTION:-0}
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
case "$unique_colors" in
    0|1) ;;
    *)
        echo "LM_BITMAP_UNIQUE_COLORS must be 0 or 1" >&2
        exit 2
        ;;
esac
case "$maintain_detail" in
    0|1) ;;
    *)
        echo "LM_BITMAP_MAINTAIN_DETAIL must be 0 or 1" >&2
        exit 2
        ;;
esac
for reduction_method_flag in "$reduction_method_1" "$reduction_method_2"; do
    case "$reduction_method_flag" in
        0|1) ;;
        *)
            echo "all LM_BITMAP_REDUCTION_METHOD_* flags must be 0 or 1" >&2
            exit 2
            ;;
    esac
done
if [ "$reduction" != "popularity" ] &&
    { [ "$reduction_method_1" -ne 1 ] || [ "$reduction_method_2" -ne 0 ]; }; then
    echo "nondefault reduction methods require LM_BITMAP_REDUCTION=popularity" >&2
    exit 2
fi
case "$allow_unmarked" in
    0|1) ;;
    *)
        echo "LM_BITMAP_ALLOW_UNMARKED must be 0 or 1" >&2
        exit 2
        ;;
esac
case "$exact_matches" in
    1) ;;
    *)
        echo "LM_BITMAP_EXACT_MATCHES is disabled in Lunar Magic 3.63 and must remain 1" >&2
        exit 2
        ;;
esac
for bitmap_other_flag in "$optimize_8x8" "$reuse_existing_8x8" "$optimize_16x16" \
    "$layer_priority" "$use_blank_8x8" "$use_blank_16x16" "$expect_map16_exhaustion"; do
    case "$bitmap_other_flag" in
        0|1) ;;
        *)
            echo "all LM_BITMAP_* Other Options flags must be 0 or 1" >&2
            exit 2
            ;;
    esac
done
validate_hex_option() {
    option_name=$1
    option_value=$2
    option_maximum=$3
    case "$option_value" in
        ''|*[!0-9a-fA-F]*)
            echo "$option_name must be an unsigned hexadecimal value" >&2
            exit 2
            ;;
    esac
    option_decimal=$(printf '%d' "0x$option_value")
    if [ "$option_decimal" -gt "$option_maximum" ]; then
        echo "$option_name exceeds its native hexadecimal range" >&2
        exit 2
    fi
}
validate_hex_option LM_BITMAP_FIRST_8X8_TILE "$first_8x8_tile" 767
validate_hex_option LM_BITMAP_BLANK_8X8_TILE "$blank_8x8_tile" 767
validate_hex_option LM_BITMAP_FIRST_MAP16_TILE "$first_map16_tile" 65535
validate_hex_option LM_BITMAP_BLANK_MAP16_TILE "$blank_map16_tile" 65535

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

set_checkbox() {
    checkbox_id=$1
    desired_state=$2
    current_state=$(wine "$helper" "$target_executable" dialog-values 2>/dev/null |
        sed -n "s/^button=$checkbox_id check=\([01]\).*/\1/p" | head -n 1)
    [ -n "$current_state" ] || {
        echo "could not read bitmap checkbox $checkbox_id" >&2
        exit 1
    }
    if [ "$current_state" -ne "$desired_state" ]; then
        wine "$helper" "$target_executable" click "$checkbox_id" >/dev/null 2>&1
    fi
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
current_level_low=$(printf '%s' "$current_level" | cut -c 1-2)
current_level_high=$(printf '%s' "$current_level" | cut -c 3-4)
current_level_value=$((0x$current_level_low + 0x$current_level_high * 256))
current_level_hex=$(printf '%03X' "$current_level_value")
wine "$helper" "$target_executable" post-command 0x238e >/dev/null 2>&1
level_dialog_submitted=0
attempt=0
while [ "$attempt" -lt 400 ]; do
    dialog_values=$(wine "$helper" "$target_executable" dialog-values 2>/dev/null |
        tr -d '\r' || true)
    if printf '%s\n' "$dialog_values" | grep -q '^button=0x0007 .*title=&No$'; then
        # A preceding conversion dirties the disposable editor buffers. Discarding them here is
        # what makes repeated captures begin from the ROM-backed level instead of accumulated
        # in-memory palette and graphics state.
        wine "$helper" "$target_executable" click 7 >/dev/null 2>&1
    fi
    if wine "$helper" "$target_executable" set-text "0x007f,$current_level_hex" \
        >/dev/null 2>&1; then
        wine "$helper" "$target_executable" click 1 >/dev/null 2>&1
        level_dialog_submitted=1
        break
    fi
    attempt=$((attempt + 1))
    sleep 0.025
done
[ "$level_dialog_submitted" -eq 1 ] || {
    echo "Lunar Magic did not open the level-number dialog within 10 seconds" >&2
    exit 1
}
sleep 0.25
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

read_buffer_chunks() {
    address=$1
    length=$2
    output=$3
    : >"$output"
    offset=0
    while [ "$offset" -lt "$length" ]; do
        remaining=$((length - offset))
        chunk_length=65536
        if [ "$remaining" -lt "$chunk_length" ]; then
            chunk_length=$remaining
        fi
        wine "$helper" "$target_executable" read "$((address + offset)),$chunk_length" \
            2>/dev/null | xxd -r -p >>"$output"
        offset=$((offset + chunk_length))
    done
}

read_buffer 0x00758dd8 1024 "$output_dir/palette-before.rgb32"
read_buffer 0x0086b7e8 65536 "$output_dir/graphics-before.bin"
read_buffer 0x009b3f58 128 "$output_dir/palette-entry-states-before.bin"
read_buffer_chunks 0x00777e58 524288 "$output_dir/map16-definitions-before.bin"

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
wine "$helper" "$target_executable" click 0x74 >/dev/null 2>&1
other_dialog_ready=0
attempt=0
while [ "$attempt" -lt 200 ]; do
    if wine "$helper" "$target_executable" list 2>/dev/null |
        grep -q 'title=Bitmap Pasting Other Options'; then
        other_dialog_ready=1
        break
    fi
    attempt=$((attempt + 1))
    sleep 0.025
done
[ "$other_dialog_ready" -eq 1 ] || {
    echo "bitmap other-options dialog was not ready within 5 seconds" >&2
    exit 1
}
set_checkbox 0x0074 "$optimize_8x8"
set_checkbox 0x0065 "$reuse_existing_8x8"
set_checkbox 0x0066 "$optimize_16x16"
set_checkbox 0x006b "$layer_priority"
set_checkbox 0x0068 "$use_blank_8x8"
set_checkbox 0x006d "$use_blank_16x16"
wine "$helper" "$target_executable" set-text "0x0067,$first_8x8_tile" >/dev/null 2>&1
wine "$helper" "$target_executable" set-text "0x0069,$blank_8x8_tile" >/dev/null 2>&1
wine "$helper" "$target_executable" set-text "0x006c,$first_map16_tile" >/dev/null 2>&1
wine "$helper" "$target_executable" set-text "0x006e,$blank_map16_tile" >/dev/null 2>&1
wine "$helper" "$target_executable" dialog-values \
    >"$output_dir/other-options.txt" 2>/dev/null
wine "$helper" "$target_executable" list '#32770' 2>/dev/null |
    sed -n '/title=Bitmap Pasting Other Options/,/title=Convert and Paste Bitmap/p' \
    >"$output_dir/other-options-windows.txt"
wine "$helper" "$target_executable" click 1 >/dev/null 2>&1
observed_first_graphics_tile=$(read_value 0x005e55e0 4)
observed_blank_graphics_tile=$(read_value 0x005e55ec 4)
observed_first_map16_tile=$(read_value 0x005e55e4 4)
observed_blank_map16_tile=$(read_value 0x005e55f0 4)
other_option_flags=$(read_value 0x005e55f4 5)
observed_layer_priority=$(read_value 0x00e27b31 1)

if [ "$reduction" = "popularity" ] || [ "$maximum_colors" -ne 128 ] ||
    [ "$maintain_detail" -ne 0 ] || [ "$reduction_method_1" -ne 1 ] ||
    [ "$reduction_method_2" -ne 0 ] || [ "$allow_unmarked" -ne 1 ] ||
    [ "$exact_matches" -ne 1 ]; then
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
    if [ "$reduction" = "popularity" ]; then
        wine "$helper" "$target_executable" select 0x69,1 >/dev/null 2>&1
    else
        wine "$helper" "$target_executable" select 0x69,0 >/dev/null 2>&1
    fi
    wine "$helper" "$target_executable" select "0x78,$((maximum_colors - 1))" >/dev/null 2>&1
    wine "$helper" "$target_executable" select "0x71,$((priority - 1))" >/dev/null 2>&1
    set_checkbox 0x006e "$unique_colors"
    set_checkbox 0x0066 "$maintain_detail"
    set_checkbox 0x006c "$reduction_method_1"
    set_checkbox 0x006d "$reduction_method_2"
    set_checkbox 0x0074 "$allow_unmarked"
    # Lunar Magic 3.63 renders 0x65 disabled. Record its persistent state but do not fabricate
    # a gesture that the native user cannot perform.
    wine "$helper" "$target_executable" dialog-values \
        >"$output_dir/color-options.txt" 2>/dev/null
    wine "$helper" "$target_executable" click 1 >/dev/null 2>&1
fi
wine "$helper" "$target_executable" list '#32770' 2>/dev/null |
    sed -n '/title=Convert and Paste Bitmap (in hex)/,/title=16x16 Tile Map Editor/p' \
    >"$output_dir/dialog.txt"
# Opening the bitmap workflow initializes and ownership-qualifies the palette usage map. Capture
# the state and active RGB32 palette immediately before acceptance, after every options gesture,
# rather than reusing the stale modeless-editor values observed before the workflow began.
read_buffer 0x009b3f58 128 "$output_dir/palette-entry-states.bin"
read_buffer 0x00758dd8 1024 "$output_dir/palette-effective.rgb32"
wine "$helper" "$target_executable" click 1 >/dev/null 2>&1
map16_exhaustion_warning=0
if [ "$expect_map16_exhaustion" -eq 1 ]; then
    warning_ready=0
    attempt=0
    while [ "$attempt" -lt 400 ]; do
        warning_windows=$(wine "$helper" "$target_executable" list '#32770' 2>/dev/null |
            tr -d '\r' || true)
        if printf '%s\n' "$warning_windows" |
            grep -q "There weren't enough blank 16x16 tiles remaining"; then
            warning_ready=1
            break
        fi
        if ! kill -0 "$paste_pid" 2>/dev/null; then
            break
        fi
        attempt=$((attempt + 1))
        sleep 0.025
    done
    [ "$warning_ready" -eq 1 ] || {
        echo "expected Map16 exhaustion warning was not ready within 10 seconds" >&2
        exit 1
    }
    printf '%s\n' "$warning_windows" >"$output_dir/map16-exhaustion-warning.txt"
    map16_exhaustion_warning=1
    wine "$helper" "$target_executable" click 1 >/dev/null 2>&1
fi
wait "$paste_pid"
paste_pid=

read_buffer 0x00758dd8 1024 "$output_dir/palette-after.rgb32"
read_buffer 0x0086b7e8 65536 "$output_dir/graphics-after.bin"
read_buffer_chunks 0x00777e58 524288 "$output_dir/map16-definitions-after.bin"

palette_differences=$(cmp -l \
    "$output_dir/palette-before.rgb32" "$output_dir/palette-after.rgb32" | wc -l | tr -d ' ')
graphics_differences=$(cmp -l \
    "$output_dir/graphics-before.bin" "$output_dir/graphics-after.bin" | wc -l | tr -d ' ')
map16_differences=$(cmp -l \
    "$output_dir/map16-definitions-before.bin" "$output_dir/map16-definitions-after.bin" |
    wc -l | tr -d ' ')
palette_before_sha=$(shasum -a 256 "$output_dir/palette-before.rgb32" | awk '{print $1}')
palette_after_sha=$(shasum -a 256 "$output_dir/palette-after.rgb32" | awk '{print $1}')
graphics_before_sha=$(shasum -a 256 "$output_dir/graphics-before.bin" | awk '{print $1}')
graphics_after_sha=$(shasum -a 256 "$output_dir/graphics-after.bin" | awk '{print $1}')
map16_before_sha=$(shasum -a 256 "$output_dir/map16-definitions-before.bin" | awk '{print $1}')
map16_after_sha=$(shasum -a 256 "$output_dir/map16-definitions-after.bin" | awk '{print $1}')
palette_entry_states_sha=$(shasum -a 256 "$output_dir/palette-entry-states.bin" | awk '{print $1}')

{
    printf 'field\tvalue\n'
    printf 'target_executable\t%s\n' "$target_executable"
    printf 'modeless_handle\t%s\n' "$modeless_handle"
    printf 'current_level_le32\t%s\n' "$current_level"
    printf 'reduction\t%s\n' "$reduction"
    printf 'priority\t%s\n' "$priority"
    printf 'maximum_colors\t%s\n' "$maximum_colors"
    printf 'unique_colors\t%s\n' "$unique_colors"
    printf 'maintain_detail\t%s\n' "$maintain_detail"
    printf 'reduction_method_1\t%s\n' "$reduction_method_1"
    printf 'reduction_method_2\t%s\n' "$reduction_method_2"
    printf 'allow_unmarked\t%s\n' "$allow_unmarked"
    printf 'exact_matches\t%s\n' "$exact_matches"
    printf 'optimize_8x8\t%s\n' "$optimize_8x8"
    printf 'reuse_existing_8x8\t%s\n' "$reuse_existing_8x8"
    printf 'optimize_16x16\t%s\n' "$optimize_16x16"
    printf 'requested_layer_priority\t%s\n' "$layer_priority"
    printf 'use_blank_8x8\t%s\n' "$use_blank_8x8"
    printf 'use_blank_16x16\t%s\n' "$use_blank_16x16"
    printf 'requested_first_8x8_tile_hex\t%s\n' "$first_8x8_tile"
    printf 'requested_blank_8x8_tile_hex\t%s\n' "$blank_8x8_tile"
    printf 'requested_first_map16_tile_hex\t%s\n' "$first_map16_tile"
    printf 'requested_blank_map16_tile_hex\t%s\n' "$blank_map16_tile"
    printf 'expected_map16_exhaustion\t%s\n' "$expect_map16_exhaustion"
    printf 'observed_map16_exhaustion_warning\t%s\n' "$map16_exhaustion_warning"
    printf 'observed_first_graphics_tile_le32\t%s\n' "$observed_first_graphics_tile"
    printf 'observed_blank_graphics_tile_le32\t%s\n' "$observed_blank_graphics_tile"
    printf 'observed_first_map16_tile_le32\t%s\n' "$observed_first_map16_tile"
    printf 'observed_blank_map16_tile_le32\t%s\n' "$observed_blank_map16_tile"
    printf 'other_option_flags_f4_through_f8\t%s\n' "$other_option_flags"
    printf 'observed_layer_priority\t%s\n' "$observed_layer_priority"
    printf 'palette_byte_differences\t%s\n' "$palette_differences"
    printf 'graphics_byte_differences\t%s\n' "$graphics_differences"
    printf 'palette_before_sha256\t%s\n' "$palette_before_sha"
    printf 'palette_after_sha256\t%s\n' "$palette_after_sha"
    printf 'graphics_before_sha256\t%s\n' "$graphics_before_sha"
    printf 'graphics_after_sha256\t%s\n' "$graphics_after_sha"
    printf 'map16_definition_byte_differences\t%s\n' "$map16_differences"
    printf 'map16_definitions_before_sha256\t%s\n' "$map16_before_sha"
    printf 'map16_definitions_after_sha256\t%s\n' "$map16_after_sha"
    printf 'palette_entry_states_sha256\t%s\n' "$palette_entry_states_sha"
} >"$output_dir/manifest.tsv"

echo "bitmap import audit: $output_dir"
echo "palette byte differences: $palette_differences"
echo "graphics byte differences: $graphics_differences"
echo "Map16 definition byte differences: $map16_differences"
