// capture.js — снимает эталонные скриншоты всех HTML-фикстур
// в headless Chromium, сохраняет в tests/reference/<name>.png
//
// Запуск:
//   cd tools/playwright
//   npm install
//   npm run install-browser
//   npm run capture
//
// После этого в tests/reference/ появятся PNG, которые сравниваются
// со скриншотами Strata (см. tests/visual/render.rs).

const { chromium } = require('playwright');
const path = require('path');
const fs = require('fs');

const ROOT = path.resolve(__dirname, '..', '..');
const FIXTURES_DIR = path.join(ROOT, 'tests', 'fixtures');
const REFERENCE_DIR = path.join(ROOT, 'tests', 'reference');

const WIDTH = 1280;
const HEIGHT = 800;

async function main() {
  if (!fs.existsSync(REFERENCE_DIR)) {
    fs.mkdirSync(REFERENCE_DIR, { recursive: true });
  }

  const fixtures = fs.readdirSync(FIXTURES_DIR)
    .filter(f => f.endsWith('.html'))
    .sort();

  console.log(`Found ${fixtures.length} fixtures:`);
  for (const f of fixtures) console.log('  -', f);

  const browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({
    viewport: { width: WIDTH, height: HEIGHT },
    deviceScaleFactor: 1,
  });

  for (const fixture of fixtures) {
    const name = fixture.replace(/\.html$/, '');
    const file = path.join(FIXTURES_DIR, fixture);
    const fileUrl = 'file://' + file;

    const page = await context.newPage();
    try {
      console.log(`\n[${name}] loading ${fileUrl}`);
      await page.goto(fileUrl, { waitUntil: 'networkidle', timeout: 15000 });
      // Подождать дополнительно для шрифтов.
      await page.waitForTimeout(500);

      const outPath = path.join(REFERENCE_DIR, `${name}.png`);
      await page.screenshot({
        path: outPath,
        fullPage: false,
        omitBackground: false,
        type: 'png',
      });
      console.log(`[${name}] saved → ${outPath}`);
    } catch (err) {
      console.error(`[${name}] ERROR:`, err.message);
    } finally {
      await page.close();
    }
  }

  await browser.close();
  console.log(`\nDone. ${fixtures.length} reference screenshots saved to ${REFERENCE_DIR}`);
}

main().catch(err => {
  console.error(err);
  process.exit(1);
});
