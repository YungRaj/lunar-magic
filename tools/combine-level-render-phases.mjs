#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";

if (process.argv.length !== 6) {
  console.error(
    "usage: tools/combine-level-render-phases.mjs OUTPUT_DIR LIVE_DIR PHASE_DIRS_CSV DIFFS_CSV",
  );
  process.exit(2);
}

const [, , outputDir, liveDir, phaseDirsArg, diffsArg] = process.argv;
const phaseDirs = phaseDirsArg.split(",");
const diffPaths = diffsArg.split(",");
if (phaseDirs.length !== 4 || diffPaths.length !== 4) {
  throw new Error("exactly four phase directories and comparisons are required");
}

const parseTsv = (file) => {
  const lines = fs.readFileSync(file, "utf8").trim().split(/\r?\n/);
  const header = lines.shift().split("\t");
  return lines.map((line) =>
    Object.fromEntries(line.split("\t").map((value, index) => [header[index], value])),
  );
};
const escapeHtml = (value) =>
  String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");

const phaseRows = diffPaths.map(parseTsv);
const liveRows = parseTsv(path.join(liveDir, "manifest.tsv"));
const liveByLevel = new Map(liveRows.map((row) => [row.level, row]));
const levels = phaseRows[0].map((row) => row.level);
const bestRows = levels.map((level) => {
  const candidates = phaseRows.map((rows, phase) => ({
    ...rows.find((row) => row.level === level),
    animation_phase: phase,
  }));
  return candidates.sort(
    (left, right) =>
      Number(left.pixels_delta_gt_8) - Number(right.pixels_delta_gt_8) ||
      Number(left.mean_absolute_channel_error) -
        Number(right.mean_absolute_channel_error),
  )[0];
});

const columns = [
  "level",
  "animation_phase",
  ...Object.keys(bestRows[0]).filter(
    (key) => key !== "level" && key !== "animation_phase",
  ),
];
fs.writeFileSync(
  path.join(outputDir, "best.tsv"),
  `${columns.join("\t")}\n${bestRows
    .map((row) => columns.map((column) => row[column]).join("\t"))
    .join("\n")}\n`,
);

const cards = bestRows
  .map((best) => {
    const level = best.level;
    const live = liveByLevel.get(level);
    const liveImage =
      live?.lunar_magic_image ?? live?.image ?? live?.path ?? live?.capture;
    const phaseImages = phaseDirs.map((directory, phase) => {
      const manifest = parseTsv(path.join(directory, "manifest.tsv"));
      const row = manifest.find((entry) => entry.level === level);
      const image = path.relative(outputDir, path.join(directory, row.image));
      const selected = phase === best.animation_phase ? " selected" : "";
      return `<figure class="phase${selected}"><figcaption>Rust phase ${phase}</figcaption><img loading="lazy" src="${escapeHtml(image)}"></figure>`;
    });
    const absoluteLive = path.isAbsolute(liveImage)
      ? liveImage
      : path.join(liveDir, liveImage);
    const livePath = path.relative(outputDir, absoluteLive);
    return `<article><h2>Level ${escapeHtml(level)} · best phase ${best.animation_phase} · Δ&gt;8 ${escapeHtml(best.pixels_delta_gt_8)} · MAE ${Number(best.mean_absolute_channel_error).toFixed(3)}</h2><div class="images"><figure class="live"><figcaption>Lunar Magic</figcaption><img loading="lazy" src="${escapeHtml(livePath)}"></figure>${phaseImages.join("")}</div></article>`;
  })
  .join("\n");

fs.writeFileSync(
  path.join(outputDir, "index.html"),
  `<!doctype html><meta charset="utf-8"><title>Phase-aware Lunar Magic render audit</title>
<style>
body{font:14px system-ui,sans-serif;margin:16px;background:#181818;color:#eee}article{margin:0 0 28px;padding:12px;background:#252525;border-radius:8px}h2{font-size:16px;margin:0 0 10px}.images{display:grid;grid-template-columns:repeat(5,minmax(260px,1fr));gap:10px;overflow-x:auto}figure{margin:0;padding:6px;background:#111;border:2px solid transparent}figure.selected{border-color:#53d769}figcaption{margin-bottom:5px}img{width:100%;height:auto;image-rendering:pixelated}
</style><h1>Phase-aware Lunar Magic render audit</h1>${cards}\n`,
);
