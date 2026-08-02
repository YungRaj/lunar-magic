#!/bin/sh
set -eu

usage() {
    echo "usage: tools/lunar-magic-layer-slot-audit.sh OUTPUT [ROM]" >&2
    echo "  ROM defaults to the retained installed SMW-US fixture" >&2
    echo "  LM_LUNAR_MAGIC_EXE overrides lm363/Lunar Magic.exe" >&2
    echo "  LM_MINGW_CC overrides i686-w64-mingw32-gcc" >&2
    exit 2
}

[ "$#" -ge 1 ] && [ "$#" -le 2 ] || usage

workspace=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
output=$1
rom=${2:-"$workspace/oracle-work/lm363/pristine-us/level-save-000/after.smc"}
lunar_magic=${LM_LUNAR_MAGIC_EXE:-"$workspace/lm363/Lunar Magic.exe"}
compiler=${LM_MINGW_CC:-i686-w64-mingw32-gcc}
temporary=$(mktemp -d "${TMPDIR:-/tmp}/lm-layer-slot-audit.XXXXXX")
helper="$temporary/wine-window-command.exe"
target_executable="Lunar Magic.exe"
lunar_magic_pid=

cleanup() {
    if [ -n "$lunar_magic_pid" ]; then
        kill "$lunar_magic_pid" 2>/dev/null || true
    fi
    wineserver -k 2>/dev/null || true
    rm -rf "$temporary"
}
trap cleanup EXIT HUP INT TERM

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
[ -f "$lunar_magic" ] || {
    echo "Lunar Magic executable does not exist: $lunar_magic" >&2
    exit 1
}
[ -f "$rom" ] || {
    echo "ROM fixture does not exist: $rom" >&2
    exit 1
}

"$compiler" -std=c11 -O2 -Wall -Wextra -Werror \
    "$workspace/tools/wine-window-command.c" -lcomctl32 -lgdi32 -o "$helper"

lunar_magic_windows=$(WINEDEBUG=-all winepath -w "$lunar_magic" 2>/dev/null)
rom_windows=$(WINEDEBUG=-all winepath -w "$rom" 2>/dev/null)
WINEDEBUG=-all wine "$lunar_magic_windows" "$rom_windows" \
    >"$temporary/lunar-magic.stdout" 2>"$temporary/lunar-magic.stderr" &
lunar_magic_pid=$!

tables=
attempt=0
while [ "$attempt" -lt 400 ]; do
    tables=$(WINEDEBUG=-all wine "$helper" "$target_executable" \
        read 0x0091f330,96 2>/dev/null || true)
    case "$tables" in
        1515*) break ;;
    esac
    tables=
    attempt=$((attempt + 1))
    sleep 0.025
done
[ -n "$tables" ] || {
    echo "Lunar Magic did not initialize its level-mode tables within 10 seconds" >&2
    exit 1
}

primary_table=$(printf '%s' "$tables" | cut -c 1-64)
alternate_table=$(printf '%s' "$tables" | cut -c 65-128)
composition_table=$(printf '%s' "$tables" | cut -c 129-192)

mkdir -p "$(dirname -- "$output")"
printf 'mode\tsplit\texpanded\troute\tprimary_additive\tprimary\talternate\tcomposition\tsource\tenabled\tadditive\thalf_color\tpriority\n' >"$output"

mode=0
while [ "$mode" -lt 32 ]; do
    if [ "$mode" -lt 18 ] || [ "$mode" -ge 30 ]; then
        position=$((mode * 2 + 1))
        end=$((position + 1))
        primary=$(printf '%s' "$primary_table" | cut -c "$position-$end")
        alternate=$(printf '%s' "$alternate_table" | cut -c "$position-$end")
        composition=$(printf '%s' "$composition_table" | cut -c "$position-$end")
        split=0
        while [ "$split" -le 1 ]; do
            arrays=$(WINEDEBUG=-all wine "$helper" "$target_executable" slot-oracle \
                "0x$primary,0x$alternate,0x$composition,$split" 2>/dev/null | tr -d '\r\n ')
            [ "${#arrays}" -eq 50 ] || {
                echo "mode $(printf '%02X' "$mode") split $split returned malformed arrays: $arrays" >&2
                exit 1
            }
            printf '%02X\t%s\t0\t-\t-\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
                "$mode" "$split" "$primary" "$alternate" "$composition" \
                "$(printf '%s' "$arrays" | cut -c 1-10)" \
                "$(printf '%s' "$arrays" | cut -c 11-20)" \
                "$(printf '%s' "$arrays" | cut -c 21-30)" \
                "$(printf '%s' "$arrays" | cut -c 31-40)" \
                "$(printf '%s' "$arrays" | cut -c 41-50)" >>"$output"
            route=0
            while [ "$route" -le 1 ]; do
                primary_additive=0
                while [ "$primary_additive" -le 1 ]; do
                    arrays=$(WINEDEBUG=-all wine "$helper" "$target_executable" \
                        slot-oracle-expanded \
                        "0x$primary,0x$alternate,0x$composition,$split,$route,$primary_additive" \
                        2>/dev/null | tr -d '\r\n ')
                    [ "${#arrays}" -eq 50 ] || {
                        echo "mode $(printf '%02X' "$mode") split $split expanded route $route additive $primary_additive returned malformed arrays: $arrays" >&2
                        exit 1
                    }
                    printf '%02X\t%s\t1\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
                        "$mode" "$split" "$route" "$primary_additive" \
                        "$primary" "$alternate" "$composition" \
                        "$(printf '%s' "$arrays" | cut -c 1-10)" \
                        "$(printf '%s' "$arrays" | cut -c 11-20)" \
                        "$(printf '%s' "$arrays" | cut -c 21-30)" \
                        "$(printf '%s' "$arrays" | cut -c 31-40)" \
                        "$(printf '%s' "$arrays" | cut -c 41-50)" >>"$output"
                    primary_additive=$((primary_additive + 1))
                done
                route=$((route + 1))
            done
            split=$((split + 1))
        done
    fi
    mode=$((mode + 1))
done

echo "Lunar Magic layer-slot oracle: $output"
