"""
Strata WPT Runner — фаза 1: список ручных тестов.

Поскольку WPT — огромная suite (~50k тестов), мы не пытаемся скачать
и прогнать всё. Вместо этого:

1) Определяем подмножество тестов, которые Strata *теоретически* может
   пройти на текущем уровне (HTML parsing, basic CSS matching, layout).
2) Каждый тест — это HTML-страница + ожидаемый результат
   (либо reference PNG, либо HTML-assertion через JS).
3) Запускаем каждый тест через Strata, сравниваем с ожиданием.

Категории WPT, которые мы пытаемся пройти:
  - html/syntax/parsing/         — корректность HTML парсера
  - css/css-color/               — named colors, hex, rgb()
  - css/css-text/                — basic text layout
  - css/selectors/               — tag, class, id, combinators
  - dom/nodes/Document-URL.html  — DOM API basics (после реализации JS)

Запуск:
  cd tools/wpt-runner
  python run_wpt.py
"""

import json
import os
import shutil
import subprocess
import sys
import time
from html import escape
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
WPT_DIR = ROOT / 'tests' / 'wpt'
WPT_RESULTS = WPT_DIR / 'results.json'
WPT_REPORT = WPT_DIR / 'report.html'

# Стратегия тестирования: для каждого WPT-теста мы запускаем Strata,
# делаем скриншот, и сравниваем с эталоном (либо сравниваем DOM).
# На текущем MVP сравниваем только "страница загрузилась без паники".

# Список простых тестов, которые мы ожидаем пройти.
# Это ручная курированная выборка, аналогичная WPT-категориям.
TESTS = [
    # (name, file, category, expected_behavior)
    {
        'name': 'html-basic-structure',
        'category': 'html/syntax',
        'file': 'html-basic-structure.html',
        'description': 'Простая HTML страница должна парситься без ошибок',
    },
    {
        'name': 'html-void-elements',
        'category': 'html/syntax',
        'file': 'html-void-elements.html',
        'description': 'Void elements (br, hr, img) не должны ломать структуру',
    },
    {
        'name': 'html-entities',
        'category': 'html/syntax',
        'file': 'html-entities.html',
        'description': 'HTML entities должны корректно декодироваться',
    },
    {
        'name': 'html-nested',
        'category': 'html/syntax',
        'file': 'html-nested.html',
        'description': 'Вложенные теги должны строиться в правильное дерево',
    },
    {
        'name': 'css-color-named',
        'category': 'css/color',
        'file': 'css-color-named.html',
        'description': 'Named colors: red, blue, green должны применяться',
    },
    {
        'name': 'css-color-hex',
        'category': 'css/color',
        'file': 'css-color-hex.html',
        'description': 'Hex colors: #rgb, #rrggbb, #rrggbbaa',
    },
    {
        'name': 'css-color-rgb',
        'category': 'css/color',
        'file': 'css-color-rgb.html',
        'description': 'rgb()/rgba() function notation',
    },
    {
        'name': 'css-selector-tag',
        'category': 'css/selectors',
        'file': 'css-selector-tag.html',
        'description': 'Tag selectors: div, p, h1',
    },
    {
        'name': 'css-selector-class',
        'category': 'css/selectors',
        'file': 'css-selector-class.html',
        'description': 'Class selectors: .class',
    },
    {
        'name': 'css-selector-id',
        'category': 'css/selectors',
        'file': 'css-selector-id.html',
        'description': 'ID selectors: #id',
    },
    {
        'name': 'css-selector-descendant',
        'category': 'css/selectors',
        'file': 'css-selector-descendant.html',
        'description': 'Descendant combinator: div p',
    },
    {
        'name': 'css-selector-child',
        'category': 'css/selectors',
        'file': 'css-selector-child.html',
        'description': 'Child combinator: div > p',
    },
    {
        'name': 'css-display-block',
        'category': 'css/display',
        'file': 'css-display-block.html',
        'description': 'display: block должны быть на отдельных строках',
    },
    {
        'name': 'css-display-none',
        'category': 'css/display',
        'file': 'css-display-none.html',
        'description': 'display: none элементы не должны рендериться',
    },
    {
        'name': 'css-flexbox-row',
        'category': 'css/flexbox',
        'file': 'css-flexbox-row.html',
        'description': 'display: flex; flex-direction: row',
    },
    {
        'name': 'css-flexbox-column',
        'category': 'css/flexbox',
        'file': 'css-flexbox-column.html',
        'description': 'display: flex; flex-direction: column',
    },
]

FIXTURES_DIR = WPT_DIR / 'fixtures'
STRATA_BIN = ROOT / 'target' / 'release' / 'strata'
if not STRATA_BIN.exists():
    STRATA_BIN = ROOT / 'target' / 'debug' / 'strata'


def render_in_strata(html_path: Path) -> dict:
    """Запустить strata-browser на файле в headless режиме, получить PNG."""
    if not STRATA_BIN.exists():
        return {
            'status': 'skipped',
            'reason': f'strata binary not found at {STRATA_BIN}',
        }
    out_png = html_path.with_suffix('.png')
    try:
        start = time.perf_counter()
        result = subprocess.run(
            [str(STRATA_BIN), '--headless', str(html_path), str(out_png)],
            capture_output=True,
            text=True,
            timeout=10,
        )
        elapsed = time.perf_counter() - start
        png_exists = out_png.exists()
        png_size = out_png.stat().st_size if png_exists else 0
        return {
            'status': 'pass' if result.returncode == 0 and png_exists and png_size > 1000 else 'fail',
            'exit_code': result.returncode,
            'stdout': result.stdout[:500],
            'stderr': result.stderr[:500],
            'elapsed_ms': int(elapsed * 1000),
            'png_size': png_size,
        }
    except subprocess.TimeoutExpired:
        return {'status': 'timeout', 'reason': 'exceeded 10s'}
    except Exception as e:
        return {'status': 'error', 'reason': str(e)}


def main():
    if not FIXTURES_DIR.exists():
        print(f'Fixtures dir not found: {FIXTURES_DIR}')
        print('Create fixture HTML files there, one per test.')
        sys.exit(1)

    results = []
    for test in TESTS:
        print(f"\n[{test['name']}] {test['description']}")
        fixture_path = FIXTURES_DIR / test['file']
        if not fixture_path.exists():
            print(f"  SKIP — fixture file missing: {fixture_path}")
            results.append({
                **test,
                'result': {'status': 'missing', 'reason': f'no file {test["file"]}'},
            })
            continue

        r = render_in_strata(fixture_path)
        print(f"  → {r['status']} ({r.get('elapsed_ms', '—')} ms)")
        if r.get('stderr'):
            print(f"    stderr: {r['stderr'][:200]}")
        results.append({**test, 'result': r})

    # Сохранить JSON.
    WPT_RESULTS.parent.mkdir(parents=True, exist_ok=True)
    with open(WPT_RESULTS, 'w') as f:
        json.dump(results, f, indent=2, ensure_ascii=False)
    print(f'\nJSON results: {WPT_RESULTS}')

    # Сгенерировать HTML отчёт.
    html = render_html(results)
    with open(WPT_REPORT, 'w') as f:
        f.write(html)
    print(f'HTML report: {WPT_REPORT}')

    # Сводка.
    by_status = {}
    for r in results:
        s = r['result']['status']
        by_status[s] = by_status.get(s, 0) + 1
    print('\n=== Summary ===')
    for status, count in sorted(by_status.items()):
        print(f'  {status}: {count}')


def render_html(results):
    rows = []
    for r in results:
        status = r['result']['status']
        cls = 'pass' if status == 'pass' else 'warn' if status == 'skipped' else 'fail'
        elapsed = r['result'].get('elapsed_ms', '—')
        rows.append(f"""
        <tr class="{cls}">
          <td>{escape(r['name'])}</td>
          <td>{escape(r['category'])}</td>
          <td>{status}</td>
          <td>{elapsed} ms</td>
          <td>{escape(r['description'])}</td>
        </tr>""")

    return f"""<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8">
  <title>Strata — WPT Results</title>
  <style>
    body {{ font-family: sans-serif; max-width: 1200px; margin: 20px auto; }}
    table {{ border-collapse: collapse; width: 100%; }}
    th, td {{ padding: 8px 12px; border: 1px solid #ddd; text-align: left; vertical-align: top; }}
    th {{ background: #f5f5f5; }}
    tr.pass {{ background: #e6ffe6; }}
    tr.warn {{ background: #fff9e6; }}
    tr.fail {{ background: #ffe6e6; }}
  </style>
</head>
<body>
  <h1>Strata — Web Platform Tests Results</h1>
  <p>Tests run: {len(results)}</p>
  <table>
    <tr><th>Name</th><th>Category</th><th>Status</th><th>Time</th><th>Description</th></tr>
    {''.join(rows)}
  </table>
</body>
</html>"""


if __name__ == '__main__':
    main()
