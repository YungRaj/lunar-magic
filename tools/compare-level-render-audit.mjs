#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import zlib from "node:zlib";

function usage() {
  console.error(
    "usage: tools/compare-level-render-audit.mjs RUST_AUDIT_DIR LIVE_REFERENCE_DIR OUTPUT.tsv",
  );
  process.exit(2);
}

if (process.argv.length !== 5) usage();
const [, , rustAuditDir, liveReferenceDir, outputPath] = process.argv;

function decodePng(filePath) {
  const bytes = fs.readFileSync(filePath);
  const signature = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);
  if (!bytes.subarray(0, 8).equals(signature)) {
    throw new Error(`not a PNG: ${filePath}`);
  }
  let cursor = 8;
  let width;
  let height;
  let bitDepth;
  let colorType;
  const idat = [];
  while (cursor < bytes.length) {
    const length = bytes.readUInt32BE(cursor);
    const kind = bytes.toString("ascii", cursor + 4, cursor + 8);
    const data = bytes.subarray(cursor + 8, cursor + 8 + length);
    cursor += length + 12;
    if (kind === "IHDR") {
      width = data.readUInt32BE(0);
      height = data.readUInt32BE(4);
      bitDepth = data[8];
      colorType = data[9];
      if (data[10] !== 0 || data[11] !== 0 || data[12] !== 0) {
        throw new Error(`unsupported PNG compression/filter/interlace: ${filePath}`);
      }
    } else if (kind === "IDAT") {
      idat.push(data);
    } else if (kind === "IEND") {
      break;
    }
  }
  if (bitDepth !== 8 || ![2, 6].includes(colorType)) {
    throw new Error(`unsupported PNG format depth=${bitDepth} type=${colorType}: ${filePath}`);
  }
  const channels = colorType === 2 ? 3 : 4;
  const stride = width * channels;
  const filtered = zlib.inflateSync(Buffer.concat(idat));
  if (filtered.length !== (stride + 1) * height) {
    throw new Error(`wrong inflated PNG length: ${filePath}`);
  }
  const pixels = Buffer.alloc(width * height * channels);
  let source = 0;
  for (let y = 0; y < height; y += 1) {
    const filter = filtered[source];
    source += 1;
    const row = y * stride;
    const previous = row - stride;
    for (let x = 0; x < stride; x += 1) {
      const raw = filtered[source + x];
      const left = x >= channels ? pixels[row + x - channels] : 0;
      const up = y > 0 ? pixels[previous + x] : 0;
      const upperLeft = y > 0 && x >= channels ? pixels[previous + x - channels] : 0;
      let value;
      if (filter === 0) {
        value = raw;
      } else if (filter === 1) {
        value = raw + left;
      } else if (filter === 2) {
        value = raw + up;
      } else if (filter === 3) {
        value = raw + Math.floor((left + up) / 2);
      } else if (filter === 4) {
        const predictor = left + up - upperLeft;
        const leftDistance = Math.abs(predictor - left);
        const upDistance = Math.abs(predictor - up);
        const upperLeftDistance = Math.abs(predictor - upperLeft);
        const paeth =
          leftDistance <= upDistance && leftDistance <= upperLeftDistance
            ? left
            : upDistance <= upperLeftDistance
              ? up
              : upperLeft;
        value = raw + paeth;
      } else {
        throw new Error(`unsupported PNG row filter ${filter}: ${filePath}`);
      }
      pixels[row + x] = value & 0xff;
    }
    source += stride;
  }
  return { width, height, channels, pixels };
}

function rgbAt(image, x, y) {
  const offset = (y * image.width + x) * image.channels;
  return [
    image.pixels[offset],
    image.pixels[offset + 1],
    image.pixels[offset + 2],
  ];
}

function rustComparisonRgb(rust, live, cropX, cropY, x, y) {
  // Lunar Magic's vertical DIB keeps a 112-pixel black tail after the 32-tile (512-pixel)
  // level surface. The Rust canvas ends at 512 pixels and the surrounding application UI must
  // not be mistaken for level pixels.
  if (live.width === 624 && x >= 512) return [0, 0, 0];
  return rgbAt(rust, cropX + x * 2, cropY + y * 2);
}

function sampledError(rust, live, cropX, cropY, sampleStep = 8) {
  let error = 0;
  const ignoredLiveRgb = process.env.LM_COMPARE_IGNORE_LIVE_RGB
    ?.split(",")
    .map(Number);
  for (let y = 0; y < live.height; y += sampleStep) {
    for (let x = 0; x < live.width; x += sampleStep) {
      const expected = rgbAt(live, x, y);
      if (
        ignoredLiveRgb?.length === 3 &&
        expected.every((value, channel) => value === ignoredLiveRgb[channel])
      ) {
        continue;
      }
      const actual = rustComparisonRgb(rust, live, cropX, cropY, x, y);
      error +=
        Math.abs(expected[0] - actual[0]) +
        Math.abs(expected[1] - actual[1]) +
        Math.abs(expected[2] - actual[2]);
    }
  }
  return error;
}

function selectCrop(rust, live) {
  if (
    (live.width === 656 && live.height === 464) ||
    (live.width === 624 && live.height === 480)
  ) {
    return [870, 338];
  }
  const forcedCrop = process.env.LM_COMPARE_VERTICAL_CROP;
  if (forcedCrop) {
    const [x, y] = forcedCrop.split(",").map(Number);
    if (Number.isInteger(x) && Number.isInteger(y)) return [x, y];
    throw new Error(`invalid LM_COMPARE_VERTICAL_CROP: ${forcedCrop}`);
  }
  let best = [870, 338];
  let bestError = Number.POSITIVE_INFINITY;
  const maximumX = rust.width - live.width * 2;
  const maximumY = rust.height - live.height * 2;
  // Vertical levels can be much taller than the captured viewport. Search the whole available
  // canvas first, then refine around the best coarse match. Restricting this to the top few rows
  // made valid renders look structurally unrelated whenever Lunar Magic opened farther down.
  for (let cropY = 0; cropY <= maximumY; cropY += 16) {
    for (let cropX = 700; cropX <= Math.min(980, maximumX); cropX += 8) {
      if (cropX + live.width * 2 > rust.width || cropY + live.height * 2 > rust.height) {
        continue;
      }
      const error = sampledError(rust, live, cropX, cropY, 16);
      if (error < bestError) {
        bestError = error;
        best = [cropX, cropY];
      }
    }
  }
  bestError = Number.POSITIVE_INFINITY;
  const coarseBest = best;
  for (let cropY = Math.max(0, coarseBest[1] - 24);
    cropY <= Math.min(maximumY, coarseBest[1] + 24);
    cropY += 2) {
    for (let cropX = Math.max(0, coarseBest[0] - 16);
      cropX <= Math.min(maximumX, coarseBest[0] + 16);
      cropX += 2) {
      const error = sampledError(rust, live, cropX, cropY);
      if (error < bestError) {
        bestError = error;
        best = [cropX, cropY];
      }
    }
  }
  return best;
}

function compareLevel(level, rustPath, livePath) {
  if (process.env.LM_COMPARE_LIVE_IMAGE) {
    livePath = process.env.LM_COMPARE_LIVE_IMAGE;
  }
  const rust = decodePng(rustPath);
  const live = decodePng(livePath);
  const [cropX, cropY] = selectCrop(rust, live);
  const requiredWidth = cropX + live.width * 2;
  const requiredHeight = cropY + live.height * 2;
  if (rust.width < requiredWidth || rust.height < requiredHeight) {
    throw new Error(
      `Rust framebuffer is too small for level ${level}: ` +
        `${rust.width}x${rust.height}, need ${requiredWidth}x${requiredHeight}`,
    );
  }
  let exactDifferences = 0;
  let overOne = 0;
  let overEight = 0;
  let channelError = 0;
  let maxChannelDelta = 0;
  let minX = live.width;
  let minY = live.height;
  let maxX = -1;
  let maxY = -1;
  let comparedPixels = 0;
  const differencePairs = new Map();
  const ignoredLiveRgb = process.env.LM_COMPARE_IGNORE_LIVE_RGB
    ?.split(",")
    .map(Number);
  for (let y = 0; y < live.height; y += 1) {
    for (let x = 0; x < live.width; x += 1) {
      const expected = rgbAt(live, x, y);
      if (
        ignoredLiveRgb?.length === 3 &&
        expected.every((value, channel) => value === ignoredLiveRgb[channel])
      ) {
        continue;
      }
      comparedPixels += 1;
      const actual = rustComparisonRgb(rust, live, cropX, cropY, x, y);
      const deltas = expected.map((value, channel) => Math.abs(value - actual[channel]));
      const pixelMax = Math.max(...deltas);
      channelError += deltas[0] + deltas[1] + deltas[2];
      maxChannelDelta = Math.max(maxChannelDelta, pixelMax);
      if (pixelMax !== 0) exactDifferences += 1;
      if (pixelMax !== 0) {
        const key = `${expected.join(",")}/${actual.join(",")}`;
        differencePairs.set(key, (differencePairs.get(key) ?? 0) + 1);
      }
      if (pixelMax > 1) overOne += 1;
      if (pixelMax > 8) {
        overEight += 1;
        minX = Math.min(minX, x);
        minY = Math.min(minY, y);
        maxX = Math.max(maxX, x);
        maxY = Math.max(maxY, y);
      }
    }
  }
  const pixels = comparedPixels;
  const meanAbsoluteChannelError = channelError / (pixels * 3);
  const bounds = overEight === 0 ? "" : `${minX},${minY}-${maxX},${maxY}`;
  const dominantDifference =
    [...differencePairs.entries()].sort((left, right) => right[1] - left[1])[0] ?? ["", 0];
  return [
    level,
    live.width,
    live.height,
    pixels,
    exactDifferences,
    overOne,
    overEight,
    meanAbsoluteChannelError.toFixed(6),
    maxChannelDelta,
    bounds,
    `${dominantDifference[0]}:${dominantDifference[1]}`,
    cropX,
    cropY,
  ];
}

const liveManifestPath = path.join(liveReferenceDir, "manifest.tsv");
const requestedLevels = new Set(
  (process.env.LM_COMPARE_LEVELS ?? "")
    .split(",")
    .map((level) => level.trim().toUpperCase())
    .filter(Boolean),
);
const records = fs
  .readFileSync(liveManifestPath, "utf8")
  .trimEnd()
  .split("\n")
  .slice(1)
  .map((line) => line.split("\t"))
  .filter(([level]) => requestedLevels.size === 0 || requestedLevels.has(level));
const output = [
  [
    "level",
    "width",
    "height",
    "pixels",
    "different_pixels",
    "pixels_delta_gt_1",
    "pixels_delta_gt_8",
    "mean_absolute_channel_error",
    "max_channel_delta",
    "delta_gt_8_bounds",
    "dominant_live_rgb/rust_rgb:count",
    "rust_crop_x",
    "rust_crop_y",
  ].join("\t"),
];
for (const [level, , liveImage] of records) {
  const rustImage = path.join(
    rustAuditDir,
    "images",
    `level-${level}-editor-screen-0.png`,
  );
  if (!fs.existsSync(rustImage)) {
    throw new Error(`missing Rust editor framebuffer: ${rustImage}`);
  }
  output.push(compareLevel(level, rustImage, liveImage).join("\t"));
}
fs.writeFileSync(outputPath, `${output.join("\n")}\n`);
console.log(`pixel comparison: ${outputPath}`);
