// compare.js — пиксельное сравнение скриншотов Strata и эталонов Chromium.
//
// Запуск:
//   cd tools/playwright
//   npm run compare
//
// Читает:
//   tests/screenshots/<name>.png  (рендер Strata, генерируется cargo test --test visual)
//   tests/reference/<name>.png   (эталон Chromium, генерируется npm run capture)
//
// Пишет:
//   tests/diff/<name>.png         (diff изображение)
//   tests/diff/report.html        (HTML отчёт с галереей)
//
// Выводит сводку в stdout: % совпадения по каждой фикстуре.

const fs = require('fs');
const path = require('path');
const { PNG } = require('pngjs');

const ROOT = path.resolve(__dirname, '..', '..');
const STRATA_DIR = path.join(ROOT, 'tests', 'screenshots');
const REFERENCE_DIR = path.join(ROOT, 'tests', 'reference');
const DIFF_DIR = path.join(ROOT, 'tests', 'diff');

const THRESHOLD = 10; // максимальная разница по каналу (0-255)

function loadPng(filePath) {
  if (!fs.existsSync(filePath)) return null;
  const buffer = fs.readFileSync(filePath);
  return PNG.sync.read(buffer);
}

function pixelDiff(a, b, threshold) {
  const w = Math.min(a.width, b.width);
  const h = Math.min(a.height, b.height);
  const diff = new PNG({ width: w, height: h });

  let matched = 0;
  let total = 0;

  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const i = (a.width * y + x) << 2;
      const j = (b.width * y + x) << 2;
      const dr = Math.abs(a.data[i] - b.data[j]);
      const dg = Math.abs(a.data[i + 1] - b.data[j + 1]);
      const db = Math.abs(a.data[i + 2] - b.data[j + 2]);
      const da = Math.abs(a.data[i + 3] - b.data[j + 3]);
      const maxDiff = Math.max(dr, dg, db, da);
      total++;

      const di = (w * y + x) << 2;
      if (maxDiff <= threshold) {
        matched++;
        // зелёный — совпало
        diff.data[di] = 0;
        diff.data[di + 1] = 255;
        diff.data[di + 2] = 0;
        diff.data[di + 3] = 255;
      } else {
        // красный с интенсивностью diff
        diff.data[di] = Math.min(maxDiff, 255);
        diff.data[di + 1] = 0;
        diff.data[di + 2] = 0;
        diff.data[di + 3] = 255;
      }
    }
  }

  return { matched, total, diff };
}

function main() {
  if (!fs.existsSync(DIFF_DIR)) fs.mkdirSync(DIFF_DIR, { recursive: true });

  const fixtures = fs.existsSync(STRATA_DIR)
    ? fs.readdirSync(STRATA_DIR).filter(f => f.endsWith('.png')).sort()
    : [];

  if (fixtures.length === 0) {
    console.error('No Strata screenshots found. Run: cargo test --test visual');
    process.exit(1);
  }

  const results = [];

  for (const file of fixtures) {
    const name = file.replace(/\.png$/, '');
    const strataPath = path.join(STRATA_DIR, file);
    const refPath = path.join(REFERENCE_DIR, file);
    const diffPath = path.join(DIFF_DIR, file);

    const strata = loadPng(strataPath);
    const reference = loadPng(refPath);

    if (!strata) {
      console.log(`[${name}] SKIP — Strata screenshot not found`);
      continue;
    }
    if (!reference) {
      console.log(`[${name}] SKIP — reference screenshot not found at ${refPath}`);
      console.log(`       Run: cd tools/playwright && npm run capture`);
      results.push({ name, status: 'no-reference', pct: null });
      continue;
    }

    const { matched, total, diff } = pixelDiff(strata, reference, THRESHOLD);
    const pct = (matched / total) * 100;

    fs.writeFileSync(diffPath, PNG.sync.write(diff));

    const status = pct >= 95 ? 'PASS' : pct >= 80 ? 'WARN' : 'FAIL';
    console.log(`[${name}] ${status} — ${pct.toFixed(2)}% match (${matched}/${total})`);
    results.push({ name, status, pct, matched, total });
  }

  // HTML отчёт.
  const html = renderHtmlReport(results);
  fs.writeFileSync(path.join(DIFF_DIR, 'report.html'), html);

  console.log(`\nReport: ${path.join(DIFF_DIR, 'report.html')}`);

  const failed = results.filter(r => r.status === 'FAIL').length;
  const warned = results.filter(r => r.status === 'WARN').length;
  const passed = results.filter(r => r.status === 'PASS').length;
  console.log(`\nSummary: ${passed} pass, ${warned} warn, ${failed} fail`);
  if (failed > 0) process.exit(1);
}

function renderHtmlReport(results) {
  const rows = results.map(r => {
    const cls = r.status === 'PASS' ? 'pass' : r.status === 'WARN' ? 'warn' : 'fail';
    const pctText = r.pct !== null ? `${r.pct.toFixed(2)}%` : '—';
    const matched = r.matched !== undefined ? `${r.matched}/${r.total}` : '—';
    return `
      <tr class="${cls}">
        <td>${r.name}</td>
        <td>${r.status}</td>
        <td>${pctText}</td>
        <td>${matched}</td>
        <td>
          <a href="../screenshots/${r.name}.png">strata</a> |
          <a href="../reference/${r.name}.png">reference</a> |
          <a href="${r.name}.png">diff</a>
        </td>
      </tr>`;
  }).join('');

  return `<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8">
  <title>Strata — Visual Regression Report</title>
  <style>
    body { font-family: sans-serif; max-width: 1000px; margin: 20px auto; }
    table { border-collapse: collapse; width: 100%; }
    th, td { padding: 8px 12px; border: 1px solid #ddd; text-align: left; }
    th { background: #f5f5f5; }
    tr.pass { background: #e6ffe6; }
    tr.warn { background: #fff9e6; }
    tr.fail { background: #ffe6e6; }
    a { color: #0066cc; }
  </style>
</head>
<body>
  <h1>Strata — Visual Regression Report</h1>
  <p>Threshold: ${THRESHOLD} per channel</p>
  <table>
    <tr>
      <th>Fixture</th>
      <th>Status</th>
      <th>Match %</th>
      <th>Pixels</th>
      <th>Links</th>
    </tr>
    ${rows}
  </table>
</body>
</html>`;
}

main();
