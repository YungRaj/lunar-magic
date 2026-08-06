#!/bin/sh
set -eu

if [ "$#" -ne 3 ]; then
    echo "usage: $0 OUTPUT_DIR 'Lunar Magic.exe' ORIGINAL_ROM" >&2
    echo "Runs an isolated Lunar Magic 3.63 Create/Apply IPS oracle under Wine." >&2
    exit 2
fi

output_dir=$1
lunar_magic_exe=$2
original_rom=$3
workspace=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
compiler=${LM_MINGW_CC:-i686-w64-mingw32-gcc}
wine_prefix="$output_dir/wine-prefix"
case_dir="$output_dir/case"
bin_dir="$output_dir/bin"
helper="$bin_dir/wine-window-command.exe"
# The helper locates a process by executable basename. Give the isolated oracle a unique name so
# an unrelated Lunar Magic session owned by the user can neither receive commands nor be closed.
target_executable="LMIpsOracle.exe"
lm_cli="$workspace/target/debug/lm-cli"
lm_pid=

[ -f "$lunar_magic_exe" ] || { echo "Lunar Magic executable does not exist: $lunar_magic_exe" >&2; exit 1; }
[ -f "$original_rom" ] || { echo "original ROM does not exist: $original_rom" >&2; exit 1; }
[ ! -e "$output_dir" ] || { echo "output path already exists: $output_dir" >&2; exit 1; }
command -v wine >/dev/null 2>&1 || { echo "wine is required" >&2; exit 1; }
command -v winepath >/dev/null 2>&1 || { echo "winepath is required" >&2; exit 1; }
command -v wineserver >/dev/null 2>&1 || { echo "wineserver is required" >&2; exit 1; }
command -v "$compiler" >/dev/null 2>&1 || { echo "$compiler is required" >&2; exit 1; }

mkdir -p "$bin_dir" "$case_dir/sysLMRestore"
"$compiler" -std=c11 -O2 -Wall -Wextra -Werror \
    "$workspace/tools/wine-window-command.c" -lcomctl32 -lgdi32 -o "$helper"
cargo build --quiet --manifest-path "$workspace/Cargo.toml" -p lm-cli

wine_run() {
    WINEPREFIX="$wine_prefix" WINEDEBUG=-all wine "$@"
}

helper_run() {
    wine_run "$helper" "$target_executable" "$@"
}

windows() {
    helper_run list 2>/dev/null || true
}

has_title() {
    windows | grep -Fq "title=$1"
}

wait_for_title() {
    wanted_title=$1
    wait_attempt=0
    while [ "$wait_attempt" -lt 300 ]; do
        if has_title "$wanted_title"; then
            return 0
        fi
        wait_attempt=$((wait_attempt + 1))
        sleep 0.1
    done
    echo "window did not appear: $wanted_title" >&2
    windows >&2
    return 1
}

wait_for_frame() {
    wait_attempt=0
    while [ "$wait_attempt" -lt 300 ]; do
        if windows | grep -Fq "class=LMFrame title=Lunar Magic"; then
            return 0
        fi
        if [ -n "$lm_pid" ] && ! kill -0 "$lm_pid" 2>/dev/null; then
            return 1
        fi
        wait_attempt=$((wait_attempt + 1))
        sleep 0.1
    done
    echo "Lunar Magic frame did not appear" >&2
    return 1
}

dialog_children() {
    helper_run children 2>/dev/null
}

set_dialog_path() {
    host_path=$1
    windows_path=$(WINEPREFIX="$wine_prefix" WINEDEBUG=-all winepath -w "$host_path")
    helper_run set-text "0x047c,$windows_path" >/dev/null 2>&1
    helper_run click 1 >/dev/null 2>&1
}

dismiss_if_present() {
    dialog_title=$1
    control_id=$2
    if has_title "$dialog_title"; then
        helper_run click "$control_id" >/dev/null 2>&1
    fi
}

stop_lunar_magic() {
    if [ -n "$lm_pid" ]; then
        # Keep the isolated Wine server alive between cases. Reinitializing its device processes
        # for every dialog sequence is both slower and less reliable than terminating only the
        # uniquely named oracle executable.
        wine_run taskkill /F /IM "$target_executable" >/dev/null 2>&1 || true
    fi
    if [ -n "$lm_pid" ]; then
        wait "$lm_pid" 2>/dev/null || true
        lm_pid=
    fi
}

cleanup() {
    stop_lunar_magic
    WINEPREFIX="$wine_prefix" WINEDEBUG=-all wineserver -k >/dev/null 2>&1 || true
}
trap cleanup EXIT HUP INT TERM

start_lunar_magic() {
    rom_path=$1
    log_path=$2
    expect_modified_warning=${3:-no}
    rom_windows=$(WINEPREFIX="$wine_prefix" WINEDEBUG=-all winepath -w "$rom_path")
    launch_attempt=0
    while [ "$launch_attempt" -lt 3 ]; do
        (
            cd "$case_dir"
            WINEPREFIX="$wine_prefix" WINEDEBUG=-all \
                wine "$target_executable" "$rom_windows" >>"$log_path" 2>&1
        ) &
        lm_pid=$!
        if wait_for_frame; then
            break
        fi
        wait "$lm_pid" 2>/dev/null || true
        lm_pid=
        launch_attempt=$((launch_attempt + 1))
        sleep 0.5
    done
    [ -n "$lm_pid" ] || {
        echo "Lunar Magic exited during startup after three attempts" >&2
        return 1
    }
    # Modified fixtures produce the original's informational open warning. It is not part of the
    # IPS operation and must be acknowledged before posting the menu command. The frame becomes
    # enumerable before this modal is created, so explicitly wait when the caller expects it.
    if [ "$expect_modified_warning" = yes ]; then
        wait_for_title "Warning: This isn't a fresh ROM!"
        helper_run click 1 >/dev/null 2>&1
    fi
    sleep 0.5
}

cp "$lunar_magic_exe" "$case_dir/$target_executable"
cp "$original_rom" "$case_dir/original-headered.smc"
cp "$original_rom" "$case_dir/sysLMRestore/smwOrig.smc"

physical_size=$(wc -c < "$case_dir/original-headered.smc" | tr -d ' ')
if [ $((physical_size % 0x8000)) -eq 512 ]; then
    dd if="$case_dir/original-headered.smc" of="$case_dir/original-logical.smc" \
        bs=512 skip=1 status=none
    head -c 512 "$case_dir/original-headered.smc" >"$case_dir/copier-header.bin"
else
    cp "$case_dir/original-headered.smc" "$case_dir/original-logical.smc"
    : >"$case_dir/copier-header.bin"
fi

"$lm_cli" patch \
    "$case_dir/original-logical.smc" "$case_dir/modified-logical-unfixed.smc" 0x1000 42
"$lm_cli" checksum-auto \
    "$case_dir/modified-logical-unfixed.smc" "$case_dir/modified-logical.smc"
{
    cat "$case_dir/copier-header.bin"
    cat "$case_dir/modified-logical.smc"
} >"$case_dir/modified-headered.smc"
"$lm_cli" ips-create \
    "$case_dir/original-headered.smc" "$case_dir/modified-headered.smc" \
    "$case_dir/rust.ips"

WINEPREFIX="$wine_prefix" WINEDEBUG=-all wineboot -u >/dev/null 2>&1
# Lunar Magic's editor and dialogs are GDI-only. Avoid initializing an unnecessary Vulkan device
# for each isolated oracle launch; this also keeps repeated headless audit runs deterministic.
wine_run reg add 'HKCU\Software\Wine\Direct3D' /v renderer /d gdi /f >/dev/null 2>&1

# Create IPS through Lunar Magic's File menu command ($23BA). The adjacent sysLMRestore copy lets
# Lunar Magic resolve its original image exactly as it does for a normal modified-ROM workspace.
start_lunar_magic "$case_dir/modified-headered.smc" "$output_dir/create.log" yes
helper_run post-command 0x23ba >/dev/null 2>&1
wait_for_title "Select IPS File to Save As"
set_dialog_path "$case_dir/lunar-magic.ips"
wait_for_title "Patch Creation Complete!"
dialog_children >"$output_dir/create-success-dialog.txt"
cmp "$case_dir/lunar-magic.ips" "$case_dir/rust.ips"
helper_run click 1 >/dev/null 2>&1

# Apply the Lunar Magic patch to a headered pristine target. Exact physical equality proves both
# logical changed ranges and copier-header preservation.
stop_lunar_magic
cp "$case_dir/original-headered.smc" "$case_dir/apply-target.smc"
start_lunar_magic "$case_dir/apply-target.smc" "$output_dir/apply.log"
helper_run post-command 0x23bb >/dev/null 2>&1
wait_for_title "Select IPS File to Use"
set_dialog_path "$case_dir/lunar-magic.ips"
wait_for_title "Patching Complete!"
dialog_children >"$output_dir/apply-success-dialog.txt"
cmp "$case_dir/apply-target.smc" "$case_dir/modified-headered.smc"
helper_run click 1 >/dev/null 2>&1

# Applying to an already modified ROM raises the recovered warning. Cancel must leave every byte
# untouched.
cp "$case_dir/apply-target.smc" "$case_dir/modified-warning-before.smc"
dismiss_if_present "Warning: This isn't a fresh ROM!" 1
helper_run post-command 0x23bb >/dev/null 2>&1
wait_for_title "This ROM has already been changed!"
dialog_children >"$output_dir/apply-modified-warning-dialog.txt"
helper_run click 2 >/dev/null 2>&1
sleep 0.5
cmp "$case_dir/apply-target.smc" "$case_dir/modified-warning-before.smc"

# A malformed input must produce the original error and leave a pristine target byte-exact.
stop_lunar_magic
cp "$case_dir/original-headered.smc" "$case_dir/malformed-target.smc"
printf 'NOT-AN-IPS' >"$case_dir/malformed.ips"
start_lunar_magic "$case_dir/malformed-target.smc" "$output_dir/apply-malformed.log"
helper_run post-command 0x23bb >/dev/null 2>&1
wait_for_title "Select IPS File to Use"
set_dialog_path "$case_dir/malformed.ips"
sleep 2
windows >"$output_dir/apply-malformed-windows.txt"
grep -Fq "title=This is not an IPS file!" "$output_dir/apply-malformed-windows.txt"
dialog_children >"$output_dir/apply-malformed-dialog.txt"
cmp "$case_dir/malformed-target.smc" "$case_dir/original-headered.smc"
helper_run click 1 >/dev/null 2>&1

stop_lunar_magic
WINEPREFIX="$wine_prefix" WINEDEBUG=-all wineserver -k >/dev/null 2>&1 || true
trap - EXIT HUP INT TERM

changed_bytes=$(cmp -l "$case_dir/original-headered.smc" "$case_dir/modified-headered.smc" | wc -l | tr -d ' ')
{
    printf 'field\tvalue\n'
    printf 'lunar_magic_version\t3.63\n'
    printf 'create_command_id\t0x23BA\n'
    printf 'apply_command_id\t0x23BB\n'
    printf 'physical_rom_bytes\t%s\n' "$physical_size"
    printf 'logical_rom_bytes\t%s\n' "$(wc -c < "$case_dir/original-logical.smc" | tr -d ' ')"
    printf 'patch_bytes\t%s\n' "$(wc -c < "$case_dir/lunar-magic.ips" | tr -d ' ')"
    printf 'physical_changed_bytes\t%s\n' "$changed_bytes"
    printf 'create_exact_match\tpass\n'
    printf 'apply_exact_physical_match\tpass\n'
    printf 'apply_modified_cancel_unchanged\tpass\n'
    printf 'apply_malformed_unchanged\tpass\n'
    for retained in original-headered modified-headered lunar-magic rust apply-target; do
        suffix=smc
        case "$retained" in
            lunar-magic|rust) suffix=ips ;;
        esac
        printf '%s_sha256\t%s\n' "$retained" \
            "$(shasum -a 256 "$case_dir/$retained.$suffix" | awk '{print $1}')"
    done
} >"$output_dir/manifest.tsv"

echo "Lunar Magic IPS audit: $output_dir"
echo "exact patch bytes: $(wc -c < "$case_dir/lunar-magic.ips" | tr -d ' ')"
echo "exact physical changed bytes: $changed_bytes"
