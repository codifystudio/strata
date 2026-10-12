//! Визуальные регрессионные тесты.
//!
//! Запуск:
//!   cargo test --test visual -- --nocapture
//!
//! Что делает:
//! 1) Рендерит каждую фикстуру из tests/fixtures/ в PNG через strata-paint
//! 2) Сохраняет PNG в tests/screenshots/<fixture>.png
//! 3) Если есть tests/reference/<fixture>.png — сравнивает с эталоном,
//!    выдаёт % совпадения и сохраняет diff в tests/diff/<fixture>.png
//!
//! Создание эталонов (reference):
//!   см. tools/playwright/ — скрипт делает скриншоты в headless Chromium.

use std::fs;
use std::path::PathBuf;

use image::{ImageBuffer, Rgba};
use strata_bench::fixtures::{diff_dir, extract_css, find, reference_dir, screenshots_dir, FIXTURES};
use strata_css::StyleSheet;
use strata_html::parse_html;
use strata_paint::Painter;

const WIDTH: u32 = 1280;
const HEIGHT: u32 = 800;

fn save_png(path: &PathBuf, rgba: &[u8], w: u32, h: u32) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::from_raw(w, h, rgba.to_vec()).expect("buffer size mismatch");
    img.save(path).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
    Ok(())
}

/// Загрузить PNG. Возвращает None если файла нет.
fn load_png(path: &PathBuf) -> Option<ImageBuffer<Rgba<u8>, Vec<u8>>> {
    if !path.exists() {
        return None;
    }
    image::open(path).ok()?.to_rgba8().into()
}

/// Pixel diff между двумя картинками.
/// Возвращает (matched_pixels, total_pixels, diff_image).
fn pixel_diff(
    a: &ImageBuffer<Rgba<u8>, Vec<u8>>,
    b: &ImageBuffer<Rgba<u8>, Vec<u8>>,
    threshold: u8,
) -> (usize, usize, ImageBuffer<Rgba<u8>, Vec<u8>>) {
    let w = a.width().min(b.width());
    let h = a.height().min(b.height());
    let mut diff = ImageBuffer::new(w, h);
    let mut matched = 0usize;
    let mut total = 0usize;

    for y in 0..h {
        for x in 0..w {
            total += 1;
            let pa = a.get_pixel(x, y);
            let pb = b.get_pixel(x, y);
            let dr = (pa[0] as i32 - pb[0] as i32).unsigned_abs();
            let dg = (pa[1] as i32 - pb[1] as i32).unsigned_abs();
            let db = (pa[2] as i32 - pb[2] as i32).unsigned_abs();
            let da = (pa[3] as i32 - pb[3] as i32).unsigned_abs();
            let max_diff = dr.max(dg).max(db).max(da);
            if max_diff <= threshold as u32 {
                matched += 1;
                diff.put_pixel(x, y, Rgba([0, 255, 0, 255])); // green = match
            } else {
                // red channel shows diff magnitude
                let intensity = (max_diff.min(255)) as u8;
                diff.put_pixel(x, y, Rgba([intensity, 0, 0, 255])); // red = diff
            }
        }
    }

    (matched, total, diff)
}

#[test]
fn render_all_fixtures() {
    let _ = fs::create_dir_all(screenshots_dir());
    let _ = fs::create_dir_all(diff_dir());

    for f in FIXTURES {
        let doc = match parse_html(f.html) {
            Ok(d) => d,
            Err(e) => {
                panic!("parse_html({}) failed: {}", f.name, e);
            }
        };
        let css = extract_css(f.html);
        let sheet = if css.trim().is_empty() {
            None
        } else {
            StyleSheet::parse(&css).ok()
        };

        let painter = Painter::new(WIDTH, HEIGHT);
        let result = painter
            .paint(&doc, sheet.as_ref())
            .unwrap_or_else(|e| panic!("paint({}) failed: {}", f.name, e));

        let shot_path = screenshots_dir().join(format!("{}.png", f.name));
        save_png(&shot_path, &result.data, result.width, result.height)
            .unwrap_or_else(|e| panic!("save_png({}) failed: {}", f.name, e));

        println!("[strata] rendered: {} → {} ({} bytes)",
            f.name,
            shot_path.display(),
            result.data.len());

        // Если есть эталон — сравнить.
        let ref_path = reference_dir().join(format!("{}.png", f.name));
        if let Some(reference) = load_png(&ref_path) {
            let strata_img: ImageBuffer<Rgba<u8>, Vec<u8>> =
                ImageBuffer::from_raw(result.width, result.height, result.data.clone())
                    .expect("buffer size");
            let (matched, total, diff) = pixel_diff(&strata_img, &reference, 10);
            let pct = (matched as f64 / total as f64) * 100.0;
            let diff_path = diff_dir().join(format!("{}.png", f.name));
            let _ = diff.save(&diff_path);
            println!("[diff] {}: {:.2}% match ({} / {} pixels) → {}",
                f.name, pct, matched, total, diff_path.display());
        } else {
            println!("[diff] {}: no reference image found at {}",
                f.name, ref_path.display());
        }
    }

    println!("\n=== Visual regression summary ===");
    println!("Strata screenshots: {}", screenshots_dir().display());
    println!("Reference (Chromium): {}", reference_dir().display());
    println!("Diff images: {}", diff_dir().display());
}

#[test]
fn render_specific_fixture() {
    // Тест для запуска одной фикстуры: cargo test render_specific_fixture -- --nocapture
    let name = std::env::var("STRATA_FIXTURE").unwrap_or_else(|_| "simple".to_string());
    let f = find(&name).unwrap_or_else(|| panic!("fixture '{}' not found", name));

    let doc = parse_html(f.html).unwrap();
    let css = extract_css(f.html);
    let sheet = if css.trim().is_empty() {
        None
    } else {
        StyleSheet::parse(&css).ok()
    };

    let painter = Painter::new(WIDTH, HEIGHT);
    let result = painter.paint(&doc, sheet.as_ref()).unwrap();

    let path = screenshots_dir().join(format!("{}.png", f.name));
    save_png(&path, &result.data, result.width, result.height).unwrap();

    println!("Rendered {} → {}", f.name, path.display());
}

#[test]
fn list_fixtures() {
    println!("Available fixtures:");
    for f in FIXTURES {
        println!("  - {} ({} bytes)", f.name, f.html.len());
    }
}
