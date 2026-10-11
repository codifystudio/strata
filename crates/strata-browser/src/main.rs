//! Strata browser — главный бинарник.
//!
//! Запуск:
//!   strata                  → открывает about:blank
//!   strata https://example.com
//!   strata example.com      → https://example.com
//!   strata "rust web browser" → поиск в DuckDuckGo

use std::env;
use std::sync::{Arc, Mutex};

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

    let arg = env::args().nth(1).unwrap_or_else(|| "about:blank".to_string());
    let url: Url = if arg == "about:blank" {
        Url::parse("about:blank")?
    } else {
        resolve_input(&arg).map_err(|e| anyhow::anyhow!("{}", e))?
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

    // 4) Запустить окно + painter.
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
