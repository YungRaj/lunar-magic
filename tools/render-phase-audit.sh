#!/bin/sh
set -eu

if [ "$#" -lt 4 ] || [ "$#" -gt 6 ]; then
    echo "usage: tools/render-phase-audit.sh OUTPUT_DIR LIVE_REFERENCE_DIR ROM LEVELS [SCREENS] [STYLES]" >&2
    exit 2
fi

output_dir=$1
live_dir=$2
rom=$3
levels=$4
screens=${5:-0}
styles=${6:-editor}
workspace=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)

mkdir -p "$output_dir"
phase_dirs=
diffs=
for phase in 0 1 2 3 4 5 6 7; do
    phase_dir="$output_dir/phase-$phase"
    diff="$output_dir/phase-$phase.tsv"
    LM_NATIVE_ANIMATION_PHASE=$phase \
        LM_RENDER_AUDIT_REFRESH=${LM_RENDER_AUDIT_REFRESH:-1} \
        LM_NATIVE_EDITOR_CELL=${LM_NATIVE_EDITOR_CELL:-16} \
        LM_NATIVE_SCREENSHOT_HEIGHT=${LM_NATIVE_SCREENSHOT_HEIGHT:-821} \
        LM_NATIVE_EDITOR_OVERLAYS=${LM_NATIVE_EDITOR_OVERLAYS:-0} \
        LM_LUNAR_MAGIC_REFERENCE_MANIFEST=${LM_LUNAR_MAGIC_REFERENCE_MANIFEST:-"$live_dir/manifest.tsv"} \
        "$workspace/tools/render-audit.sh" "$phase_dir" "$rom" "$levels" "$screens" "$styles"
    LM_COMPARE_LEVELS=$levels \
        node "$workspace/tools/compare-level-render-audit.mjs" "$phase_dir" "$live_dir" "$diff"
    phase_dirs="${phase_dirs:+$phase_dirs,}$phase_dir"
    diffs="${diffs:+$diffs,}$diff"
done

node "$workspace/tools/combine-level-render-phases.mjs" \
    "$output_dir" "$live_dir" "$phase_dirs" "$diffs"
echo "phase-aware render audit: $output_dir/index.html"
echo "best phase comparison: $output_dir/best.tsv"
