#!/bin/sh
set -eu

if [ "$#" -ne 3 ]; then
    echo "usage: $0 OUTPUT_DIR INPUT.set INPUT.map" >&2
    echo "Run only against a disposable Lunar Magic 3.63 process with a vanilla ROM loaded." >&2
    exit 2
fi

output_dir=$1
graphics_set=$2
screen_map=$3
workspace=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
target_executable=${LM_WINE_EXECUTABLE:-LMSnesTilesetOracle.exe}
compiler=${LM_MINGW_CC:-i686-w64-mingw32-gcc}
helper="$output_dir/bin/wine-window-command.exe"
key_helper="$output_dir/bin/wine-window-key.exe"

[ -f "$graphics_set" ] || { echo "graphics set does not exist: $graphics_set" >&2; exit 1; }
[ -f "$screen_map" ] || { echo "screen map does not exist: $screen_map" >&2; exit 1; }
[ "$(wc -c < "$screen_map" | tr -d ' ')" -eq 2048 ] || {
    echo "screen map must contain exactly 2048 bytes" >&2
    exit 1
}
[ ! -e "$output_dir" ] || { echo "output path already exists: $output_dir" >&2; exit 1; }
command -v wine >/dev/null 2>&1 || { echo "wine is required" >&2; exit 1; }
command -v winepath >/dev/null 2>&1 || { echo "winepath is required" >&2; exit 1; }
command -v "$compiler" >/dev/null 2>&1 || { echo "$compiler is required" >&2; exit 1; }

mkdir -p "$output_dir/bin"
"$compiler" -std=c11 -O2 -Wall -Wextra -Werror \
    "$workspace/tools/wine-window-command.c" -lcomctl32 -lgdi32 -o "$helper"
"$compiler" -std=c11 -O2 -Wall -Wextra -Werror \
    "$workspace/tools/wine-window-key.c" -o "$key_helper"

read_value() {
    wine "$helper" "$target_executable" read "$1,$2" 2>/dev/null | tr -d '\r\n'
}

write_hex() {
    write_address=$1
    write_bytes=$2
    write_index=0
    while [ "$write_index" -lt "$((${#write_bytes} / 2))" ]; do
        write_byte=$(printf '%s' "$write_bytes" | cut -c "$((write_index * 2 + 1))-$((write_index * 2 + 2))")
        wine "$helper" "$target_executable" write-byte \
            "$((write_address + write_index)),0x$write_byte" >/dev/null 2>&1
        write_index=$((write_index + 1))
    done
}

wait_for_title() {
    wanted_title=$1
    wait_attempt=0
    while [ "$wait_attempt" -lt 200 ]; do
        if wine "$helper" "$target_executable" list 2>/dev/null | grep -Fq "title=$wanted_title"; then
            return 0
        fi
        wait_attempt=$((wait_attempt + 1))
        sleep 0.025
    done
    echo "window did not appear: $wanted_title" >&2
    return 1
}

[ "$(read_value 0x00e2782a 1)" = "01" ] || {
    echo "the target process does not have a ROM level loaded" >&2
    exit 1
}
wine "$helper" "$target_executable" write-byte 0x00e27828,0 >/dev/null 2>&1
wine "$helper" "$target_executable" post-command 0x232f >/dev/null 2>&1
wait_for_title "16x16 Tile Map Editor"

# The importer is Lunar Magic's intentionally hidden Ctrl+Shift+Alt+F1+Insert command. Temporarily
# neutralize only its four modifier-state exits in this disposable process, post Insert to the
# authenticated Map16 render child, then immediately restore every original instruction byte.
write_hex 0x00500375 909090909090
write_hex 0x00500382 909090909090
write_hex 0x0050038f 909090909090
write_hex 0x0050039c 909090909090
wine "$key_helper" "$target_executable" @0x009b958c 0x2d
wait_for_title "Import SNES Tile Map Screen to Current Map16 Page (in hex)"
write_hex 0x00500375 0f89a5fbffff
write_hex 0x00500382 0f8998fbffff
write_hex 0x0050038f 0f898bfbffff
write_hex 0x0050039c 0f897efbffff

# Capture and select direct placement. The optimized negative path is separately documented by
# the retained run because the original mutates graphics before reporting insufficient blanks.
wine "$helper" "$target_executable" dialog-values >"$output_dir/options-default.txt" 2>/dev/null
wine "$helper" "$target_executable" click 0x70 >/dev/null 2>&1
wine "$helper" "$target_executable" dialog-values >"$output_dir/options-direct.txt" 2>/dev/null

selected_raw=$(read_value 0x00e27ae0 4)
selected_le=$(printf '%s' "$selected_raw" | cut -c 7-8)$(printf '%s' "$selected_raw" | cut -c 5-6)$(printf '%s' "$selected_raw" | cut -c 3-4)$(printf '%s' "$selected_raw" | cut -c 1-2)
selected_value=$(printf '%d' "0x$selected_le")
selected_page=$((selected_value >> 4))
map16_address=$((0x00777e58 + selected_page * 0x800))

read_buffer() {
    wine "$helper" "$target_executable" read "$1,$2" 2>/dev/null | xxd -r -p >"$3"
}
read_buffer 0x006204b0 65536 "$output_dir/graphics-before.bin"
read_buffer "$map16_address" 2048 "$output_dir/map16-before.bin"

wine "$helper" "$target_executable" click 1 >/dev/null 2>&1
wait_for_title "Select GFX Set to Use"
graphics_windows=$(winepath -w "$graphics_set")
wine "$helper" "$target_executable" set-text "0x047c,$graphics_windows" >/dev/null 2>&1
wine "$helper" "$target_executable" click 1 >/dev/null 2>&1
wait_for_title "Select Tile Map Data to Use"
map_windows=$(winepath -w "$screen_map")
wine "$helper" "$target_executable" set-text "0x047c,$map_windows" >/dev/null 2>&1
wine "$helper" "$target_executable" click 1 >/dev/null 2>&1

read_buffer 0x006204b0 65536 "$output_dir/graphics-after.bin"
read_buffer "$map16_address" 2048 "$output_dir/map16-after.bin"
graphics_differences=$(cmp -l "$output_dir/graphics-before.bin" "$output_dir/graphics-after.bin" | wc -l | tr -d ' ')
map16_differences=$(cmp -l "$output_dir/map16-before.bin" "$output_dir/map16-after.bin" | wc -l | tr -d ' ')

{
    printf 'field\tvalue\n'
    printf 'target_executable\t%s\n' "$target_executable"
    printf 'selected_page_hex\t%X\n' "$selected_page"
    printf 'map16_address_hex\t%X\n' "$map16_address"
    printf 'graphics_set_bytes\t%s\n' "$(wc -c < "$graphics_set" | tr -d ' ')"
    printf 'screen_map_bytes\t%s\n' "$(wc -c < "$screen_map" | tr -d ' ')"
    printf 'graphics_byte_differences\t%s\n' "$graphics_differences"
    printf 'map16_byte_differences\t%s\n' "$map16_differences"
    for retained in graphics-before graphics-after map16-before map16-after; do
        printf '%s_sha256\t%s\n' "$retained" \
            "$(shasum -a 256 "$output_dir/$retained.bin" | awk '{print $1}')"
    done
} >"$output_dir/manifest.tsv"

echo "SNES tileset import audit: $output_dir"
echo "graphics byte differences: $graphics_differences"
echo "Map16 byte differences: $map16_differences"
