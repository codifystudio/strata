# Strata

Независимый браузерный движок на Rust. Не использует системные WebView (WebView2/WKWebView/WebKitGTK) — весь стек HTML/CSS/Layout/Paint/JS реализован на чистом Rust.

> ⚠️ **Стадия:** активная разработка MVP. Реальные сайты пока не открывает. Цель — собственный движок для независимости браузера Sierra.

## Архитектура

```
strata/
├── crates/
│   ├── strata-core     — координатор: страница, навигация, pipeline
│   ├── strata-network  — HTTP/HTTPS клиент (reqwest + rustls)
│   ├── strata-dom      — DOM дерево, Node, Element
│   ├── strata-html     — HTML парсер → DOM
│   ├── strata-css      — CSS парсер + селекторы (tag/class/id/combinators)
│   ├── strata-layout   — layout engine (taffy: flexbox + block)
│   ├── strata-paint    — software renderer (tiny-skia)
│   ├── strata-js       — JS движок (boa)
│   ├── strata-window   — окно (winit + softbuffer)
│   ├── strata-browser  — основной бинарник браузера
│   └── strata-bench    — бенчмарки и visual regression тесты
├── tests/
│   ├── fixtures/       — 10 HTML страниц для тестирования
│   ├── screenshots/    — рендеры Strata (PNG)
│   ├── reference/      — эталонные скриншоты Chromium (PNG)
│   ├── diff/           — pixel diff между Strata и Chromium
│   ├── visual/         — integration test (cargo test)
│   └── wpt/            — Web Platform Tests раннер
├── tools/
│   ├── playwright/     — Node.js: эталонные скриншоты в Chromium
│   └── wpt-runner/     — Python: раннер Web Platform Tests
└── Cargo.toml          — workspace
```

## Pipeline рендеринга

```
URL
 │
 ▼
strata-network   ──► fetch(url) ──► bytes
 │
 ▼
strata-html     ──► parser       ──► DOM
 │
 ▼
strata-css      ──► parser+match ──► DeclaredStyle per NodeId
 │
 ▼
strata-layout   ──► taffy       ──► Layout Tree (x,y,w,h)
 │
 ▼
strata-paint    ──► tiny-skia   ──► pixels (RGBA)
 │
 ▼
strata-window   ──► softbuffer  ──► screen
```

## Запуск

### Браузер

```bash
# Открыть about:blank в окне
cargo run --release -p strata-browser

# Открыть URL
cargo run --release -p strata-browser -- https://example.com

# Открыть локальный HTML файл
cargo run --release -p strata-browser -- page.html

# Headless рендер в PNG (без окна)
cargo run --release -p strata-browser -- --headless page.html out.png

# Справка
cargo run --release -p strata-browser -- --help
```

## Тестирование

### 1. Unit-тесты

```bash
cargo test --workspace --lib
```

14 unit-тестов покрывают HTML парсер, CSS matcher, layout builder, paint.

### 2. Бенчмарки (Criterion)

```bash
# Все бенчмарки
cargo bench -p strata-bench

# По одному компоненту
cargo bench -p strata-bench --bench html_parse
cargo bench -p strata-bench --bench css_parse
cargo bench -p strata-bench --bench layout_compute
cargo bench -p strata-bench --bench paint_render
cargo bench -p strata-bench --bench full_pipeline

# Быстрый прогон (для CI)
cargo bench -p strata-bench -- --quick --warm-up-time 1 --measurement-time 2
```

Бенчмарки генерируют HTML-отчёты в `target/criterion/report/index.html`.

### 3. Визуальные регрессионные тесты

Шаг 1 — рендерим все фикстуры через Strata:
```bash
cargo test -p strata-bench --test visual -- --nocapture
# → tests/screenshots/*.png
```

Шаг 2 — снимаем эталонные скриншоты в Chromium (нужен Node.js):
```bash
cd tools/playwright
npm install
npm run install-browser  # скачать Chromium
npm run capture          # → tests/reference/*.png
```

Шаг 3 — pixel diff:
```bash
npm run compare
# → tests/diff/*.png + tests/diff/report.html
# Вывод: % совпадения для каждой фикстуры
```

Текущий результат сравнения Strata vs Chromium:
- **6 PASS** (>95% совпадения): simple, entities, forms, selectors, acid2, text-heavy
- **1 WARN** (80-95%): colors
- **3 FAIL** (<80%): flexbox, nested, semantics — известные проблемы с inline layout

### 4. Web Platform Tests (WPT)

```bash
# Убедиться, что strata собран в release
cargo build --release -p strata-browser

# Запустить 16 WPT-тестов
python3 tools/wpt-runner/run_wpt.py
# → tests/wpt/report.html
# → tests/wpt/results.json
```

Текущий результат: **16/16 PASS** (все базовые тесты HTML/CSS/selectors).

## Текущие метрики

### Производительность (criterion, release build)

| Компонент | Throughput | Latency |
|---|---|---|
| HTML parser | 125–370 MiB/s | 2–5 µs на страницу |
| CSS compute | — | 1–5 µs на страницу |
| Layout build | — | измеряется |
| Paint (1280×800) | 1.12–1.27 Gpixel/s | 800–900 µs |
| Full pipeline | 600 KB/s – 1 MB/s | 850–925 µs |

### Корректность

| Тест | Результат |
|---|---|
| Unit-тесты | 14/14 PASS |
| WPT smoke | 16/16 PASS |
| Visual regression (vs Chromium) | 6 PASS, 1 WARN, 3 FAIL |

## Известные ограничения

1. **Шрифты** — текст рисуется чёрными прямоугольниками (placeholder). Нужно подключить `cosmic-text` или `fontdue`.
2. **Inline layout** — taffy не умеет inline-flow, тексты внутри `<p>` раскладываются как block.
3. **JS DOM API** — `boa_engine` подключён, но нет `document.getElementById` и пр.
4. **Images** — `image` crate подключён, но нет декодирования `<img src>` в layout.
5. **Полная поддержка CSS** — нет `calc()`, `var()`, медиа-запросов, `position: absolute/fixed`.

## Лицензия

MIT
