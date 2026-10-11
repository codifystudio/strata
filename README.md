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
│   ├── strata-html     — HTML парсер (html5ever → DOM)
│   ├── strata-css      — CSS парсер + селекторы (cssparser + selectors)
│   ├── strata-layout   — layout engine (taffy: flexbox + block)
│   ├── strata-paint    — software renderer (tiny-skia)
│   ├── strata-js       — JS движок (boa)
│   ├── strata-window   — окно (winit + softbuffer)
│   └── strata-browser  — основной бинарник браузера
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
strata-html     ──► html5ever   ──► DOM
 │
 ▼
strata-css      ──► cssparser   ──► StyleSheet
 │                                  + selectors → matching
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

```bash
cargo run --release -p strata-browser -- https://example.com
```

## Лицензия

MIT
