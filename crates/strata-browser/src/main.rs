//! Strata browser — главный бинарник.
//!
//! Запуск:
//!   strata                                  → открывает about:blank в окне
//!   strata https://example.com              → открывает URL в окне
//!   strata example.com                      → https://example.com
//!   strata "rust web browser"               → поиск в DuckDuckGo
//!   strata --headless file.html             → рендерит в PNG, не открывает окно
//!   strata --headless file.html out.png     → рендерит и сохраняет в PNG

use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use strata_core::url::resolve_input;
use strata_css::StyleSheet;
use strata_html::parse_html;
use strata_network::NetworkClient;
use strata_paint::Painter;
use strata_window::{RenderRequest, WindowApp};
use url::Url;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args: Vec<String> = env::args().skip(1).collect();

    // Парсинг флагов.
    let mut headless = false;
    let mut out_png: Option<PathBuf> = None;
    let mut input_args: Vec<String> = Vec::new();

    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--headless" | "-H" => headless = true,
            "--help" | "-h" => {
                print_usage();
                return Ok(());
            }
            "--version" | "-V" => {
                println!("strata {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "--out" | "-o" => {
                if let Some(p) = iter.next() {
                    out_png = Some(PathBuf::from(p));
                }
            }
            _ => input_args.push(arg),
        }
    }

    let input = input_args.first().cloned().unwrap_or_else(|| "about:blank".to_string());

    // Если в input_args есть второй аргумент и нет --out, считаем что это out png.
    if out_png.is_none() && input_args.len() > 1 {
        out_png = Some(PathBuf::from(&input_args[1]));
    }

    let url: Url = if input == "about:blank" {
        Url::parse("about:blank")?
    } else if input.starts_with("file://") || input.starts_with('/') || input.ends_with(".html") {
        // Локальный файл.
        let path = if input.starts_with("file://") {
            PathBuf::from(input.trim_start_matches("file://"))
        } else {
            PathBuf::from(&input)
        };
        let abs = fs::canonicalize(&path).unwrap_or(path);
        Url::parse(&format!("file://{}", abs.display()))?
    } else {
        resolve_input(&input).map_err(|e| anyhow::anyhow!("{}", e))?
    };

    log::info!("Strata: loading {}", url);

    // 1) Загрузить HTML.
    let html = if url.scheme() == "about" {
        r#"<!DOCTYPE html>
<html>
<head><title>Strata</title></head>
<body>
  <h1>Strata</h1>
  <p>Независимый браузерный движок на Rust.</p>
  <p>Запустите с URL аргументом: <code>strata https://example.com</code></p>
</body>
</html>"#
        .to_string()
    } else if url.scheme() == "file" {
        let path = url.to_file_path().map_err(|_| anyhow::anyhow!("invalid file path"))?;
        fs::read_to_string(&path)?
    } else {
        let net = NetworkClient::new().map_err(|e| anyhow::anyhow!("{}", e))?;
        let resp = net.fetch(&url).await.map_err(|e| anyhow::anyhow!("{}", e))?;
        String::from_utf8_lossy(&resp.bytes).into_owned()
    };

    // 2) Распарсить HTML.
    let doc = parse_html(&html).map_err(|e| anyhow::anyhow!("{}", e))?;

    // 3) Собрать <style> теги в один author stylesheet.
    let mut css = String::new();
    for style_node in doc.find_by_tag("style") {
        css.push_str(&doc.text_content(style_node));
        css.push('\n');
    }
    let author_sheet = if css.trim().is_empty() {
        None
    } else {
        match StyleSheet::parse(&css) {
            Ok(s) => Some(s),
            Err(e) => {
                log::warn!("CSS parse error: {}", e);
                None
            }
        }
    };

    // 4) Headless mode — рендер в PNG без окна.
    if headless {
        let width: u32 = 1280;
        let height: u32 = 800;
        let painter = Painter::new(width, height);
        let result = painter
            .paint(&doc, author_sheet.as_ref())
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        let out_path = out_png.unwrap_or_else(|| PathBuf::from("strata-output.png"));
        save_png(&out_path, &result.data, result.width, result.height)?;
        log::info!("Saved: {} ({}x{})", out_path.display(), result.width, result.height);
        return Ok(());
    }

    // 5) Windowed mode.
    let doc = Arc::new(doc);
    let sheet = Arc::new(author_sheet);

    let mut app = WindowApp::new(format!("Strata — {}", url), 1200, 800);

    let doc_clone = doc.clone();
    let sheet_clone = sheet.clone();
    let renderer: RenderRequest = Box::new(move |w: u32, h: u32| -> Vec<u8> {
        let painter = Painter::new(w, h);
        match painter.paint(&doc_clone, sheet_clone.as_ref().as_ref()) {
            Ok(r) => r.data,
            Err(e) => {
                log::error!("paint error: {}", e);
                vec![0; (w as usize) * (h as usize) * 4]
            }
        }
    });
    app.set_renderer(renderer);

    app.run().map_err(|e| anyhow::anyhow!("{}", e))?;

    Ok(())
}

fn save_png(path: &PathBuf, rgba: &[u8], w: u32, h: u32) -> Result<()> {
    use image::{ImageBuffer, Rgba};
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::from_raw(w, h, rgba.to_vec()).ok_or_else(|| anyhow::anyhow!("buffer size"))?;
    img.save(path)?;
    Ok(())
}

fn print_usage() {
    eprintln!(
        "strata {version} — independent browser engine\n\
\n\
Usage:\n\
  strata [OPTIONS] [URL_OR_FILE] [OUTPUT_PNG]\n\
\n\
Options:\n\
  -H, --headless       Render to PNG without opening a window\n\
  -o, --out <PATH>     Output PNG path (for --headless)\n\
  -h, --help           Show this help\n\
  -V, --version        Show version\n\
\n\
Examples:\n\
  strata                              # open about:blank in window\n\
  strata https://example.com          # open URL in window\n\
  strata example.com                  # https://example.com\n\
  strata page.html                    # open local HTML file\n\
  strata --headless page.html out.png # render to PNG, no window\n\
  strata --headless https://example.com\n",
        version = env!("CARGO_PKG_VERSION")
    );
}
