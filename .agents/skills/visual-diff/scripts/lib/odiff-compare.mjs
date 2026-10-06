/**
 * odiff-compare.mjs - native image diff via odiff-bin.
 *
 * Wraps odiff's `compare()` into a normalized result and applies the repo's
 * Bricks-transfer acceptance gate (diff < 0.1%). odiff is far faster than the
 * old pixelmatch path on full-page screenshots and returns machine-readable
 * numbers so the batch runner can report without reading the images.
 *
 * Size mismatch (e.g. local vs live full-page heights) is handled by padding
 * both images to a shared canvas before diffing - the padded region shows up as
 * difference, so a height mismatch correctly raises the diff %, instead of
 * odiff bailing out with a hard `layout-diff`.
 */

import { compare } from 'odiff-bin';
import { PNG } from 'pngjs';
import { existsSync, openSync, readSync, closeSync, readFileSync, writeFileSync, unlinkSync, createReadStream } from 'fs';

/** Bricks transfer acceptance gate: pass when < this many percent of pixels differ. */
export const THRESHOLD_PCT = 0.1;

/** Read width/height from a PNG's IHDR header (24 bytes) without decoding the image. */
function pngSize(path) {
  const fd = openSync(path, 'r');
  try {
    const buf = Buffer.alloc(24);
    readSync(fd, buf, 0, 24, 0);
    return { width: buf.readUInt32BE(16), height: buf.readUInt32BE(20) };
  } finally {
    closeSync(fd);
  }
}

/**
 * Re-encode a PNG in place via pngjs. odiff's diff output carries chunks that
 * the vision/Read API can't decode; round-tripping through pngjs strips them to
 * a clean RGB PNG so the diff image is actually viewable. Best-effort.
 */
function reencodePng(path) {
  return new Promise((resolve) => {
    const src = createReadStream(path).pipe(new PNG());
    src.on('parsed', function () {
      try {
        const out = new PNG({ width: this.width, height: this.height, colorType: 2 });
        for (let i = 0; i < this.data.length; i += 4) {
          const a = this.data[i + 3] / 255;
          out.data[i] = Math.round(this.data[i] * a + 255 * (1 - a));
          out.data[i + 1] = Math.round(this.data[i + 1] * a + 255 * (1 - a));
          out.data[i + 2] = Math.round(this.data[i + 2] * a + 255 * (1 - a));
          out.data[i + 3] = 255;
        }
        writeFileSync(path, PNG.sync.write(out));
      } catch { /* keep odiff's original on failure */ }
      resolve();
    });
    src.on('error', () => resolve()); // trailing-chunk quirk fires after 'parsed'; ignore
  });
}

/** Write a copy of `img` padded onto a w×h mid-grey canvas. Returns the new path. */
function writePadded(img, w, h, outPath) {
  const out = new PNG({ width: w, height: h });
  out.data.fill(128); // mid-grey so padded regions are visible in the diff
  PNG.bitblt(img, out, 0, 0, img.width, img.height, 0, 0);
  writeFileSync(outPath, PNG.sync.write(out));
  return outPath;
}

/**
 * Compare two PNGs and write a highlighted diff image.
 *
 * @param {string} basePath      Reference image (e.g. local).
 * @param {string} comparePath   Candidate image (e.g. live WordPress).
 * @param {string} diffPath      Where to write the highlighted diff PNG.
 * @param {object} [opts]
 * @param {number} [opts.threshold=0.1]  Per-pixel color tolerance (0–1).
 * @param {boolean} [opts.antialiasing=true] Don't count anti-aliased pixels.
 * @param {string} [opts.diffColor='#ff00ff'] Highlight color for changed pixels.
 * @param {number} [opts.gatePct]  Acceptance gate percent (default THRESHOLD_PCT).
 * @returns {Promise<{pass:boolean, diffPercentage:number, diffCount:number,
 *   reason:string|null, message:string, diffPath:string|null, sizeMismatch:boolean}>}
 */
export async function odiffCompare(basePath, comparePath, diffPath, opts = {}) {
  const {
    threshold = 0.1,
    antialiasing = true,
    diffColor = '#ff00ff',
    gatePct = THRESHOLD_PCT,
  } = opts;

  for (const [label, p] of [['base', basePath], ['compare', comparePath]]) {
    if (!existsSync(p)) {
      return {
        pass: false, diffPercentage: 100, diffCount: 0, reason: 'file-not-exists',
        message: `${label} file not found: ${p}`, diffPath: null, sizeMismatch: false,
      };
    }
  }

  // Pad to a shared canvas when dimensions differ (fast path: header read only).
  const a = pngSize(basePath);
  const b = pngSize(comparePath);
  const sizeMismatch = a.width !== b.width || a.height !== b.height;
  let baseP = basePath;
  let compP = comparePath;
  const temps = [];
  if (sizeMismatch) {
    const w = Math.max(a.width, b.width);
    const h = Math.max(a.height, b.height);
    const imgA = PNG.sync.read(readFileSync(basePath));
    const imgB = PNG.sync.read(readFileSync(comparePath));
    if (imgA.width !== w || imgA.height !== h) {
      baseP = basePath.replace(/\.png$/, '') + '.pad.png';
      writePadded(imgA, w, h, baseP);
      temps.push(baseP);
    }
    if (imgB.width !== w || imgB.height !== h) {
      compP = comparePath.replace(/\.png$/, '') + '.pad.png';
      writePadded(imgB, w, h, compP);
      temps.push(compP);
    }
  }

  let result;
  try {
    result = await compare(baseP, compP, diffPath, {
      threshold,
      antialiasing,
      diffColor,
      noFailOnFsErrors: true,
    });
  } finally {
    for (const t of temps) { try { unlinkSync(t); } catch { /* ignore */ } }
  }

  const sizeNote = sizeMismatch
    ? ` (size mismatch ${a.width}x${a.height} vs ${b.width}x${b.height})`
    : '';

  // Identical images: odiff returns { match: true } and writes no diff file.
  if (result.match) {
    return {
      pass: true, diffPercentage: 0, diffCount: 0, reason: null,
      message: 'identical', diffPath: null, sizeMismatch,
    };
  }

  if (result.reason === 'file-not-exists') {
    return {
      pass: false, diffPercentage: 100, diffCount: 0, reason: 'file-not-exists',
      message: `file not found: ${result.file}`, diffPath: null, sizeMismatch,
    };
  }

  const diffPercentage = result.diffPercentage ?? (result.reason === 'layout-diff' ? 100 : 0);
  const pass = diffPercentage < gatePct;

  // Re-encode the diff image to a vision-API-readable PNG, but only for failures
  // (the only rows we actually open) to avoid the cost on passing comparisons.
  if (!pass && existsSync(diffPath)) await reencodePng(diffPath);

  return {
    pass,
    diffPercentage,
    diffCount: result.diffCount ?? 0,
    reason: result.reason ?? 'pixel-diff',
    message: `${diffPercentage.toFixed(2)}% pixels differ${sizeNote}`,
    diffPath,
    sizeMismatch,
  };
}
