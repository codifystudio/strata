//! Strata paint — software-рендеринг layout tree через tiny-skia.
//!
//! MVP: отрисовывает прямоугольники с background-color, рамки border-color,
//! текст через PathBuilder + fill (пока без шрифтового движка — только
//! placeholder'ы: цветной прямоугольник с фоном и текстом-чёрным блоком).

use ahash::AHashMap;
use strata_core::{StrataError, StrataResult};
use strata_css::{DeclaredStyle, StyleSheet, UA_STYLESHEET};
use strata_dom::{Document, NodeId, NodeKind};
use strata_layout::LayoutTree;
use tiny_skia::*;

/// Результат: пиксели RGBA + размер.
pub struct PaintResult {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>, // RGBA
}

pub struct Painter {
    width: u32,
    height: u32,
}

impl Painter {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    pub fn paint(
        &self,
        doc: &Document,
        author_sheet: Option<&StyleSheet>,
    ) -> StrataResult<PaintResult> {
        let mut pixmap = Pixmap::new(self.width, self.height)
            .ok_or_else(|| StrataError::Paint("pixmap alloc failed".into()))?;

        // Заполнить белым.
        pixmap.fill(Color::WHITE);

        // Слить UA + авторские стили.
        let ua = StyleSheet::parse(UA_STYLESHEET)
            .map_err(|e| StrataError::Paint(format!("UA parse: {}", e)))?;
        let ua_styles = ua.compute(doc);
        let author_styles = author_sheet
            .map(|s| s.compute(doc))
            .unwrap_or_default();

        // Построить layout tree.
        let tree = LayoutTree::build(
            doc,
            author_sheet,
            taffy::geometry::Size {
                width: self.width,
                height: self.height,
            },
        )?;

        // Обойти layout tree, отрисовав каждый узел.
        self.paint_node(doc, &tree, &ua_styles, &author_styles, tree.root, &mut pixmap, 0.0, 0.0);

        Ok(PaintResult {
            width: self.width,
            height: self.height,
            data: pixmap.data().to_vec(),
        })
    }

    fn paint_node(
        &self,
        doc: &Document,
        tree: &LayoutTree,
        ua: &AHashMap<NodeId, DeclaredStyle>,
        author: &AHashMap<NodeId, DeclaredStyle>,
        taffy_node: taffy::NodeId,
        pixmap: &mut Pixmap,
        offset_x: f32,
        offset_y: f32,
    ) {
        // Найти entry по taffy_node.
        let entry = match tree.entries.iter().find(|e| e.taffy_node == taffy_node) {
            Some(e) => e,
            None => return,
        };

        let layout = match tree.taffy.layout(taffy_node) {
            Ok(l) => l,
            Err(_) => return,
        };

        let x = offset_x + layout.location.x;
        let y = offset_y + layout.location.y;
        let w = layout.size.width;
        let h = layout.size.height;

        // Стиль узла (если это element).
        let node = doc.node(entry.node);
        if node.kind == NodeKind::Element {
            // Слить стили.
            let mut merged: AHashMap<String, String> = AHashMap::new();
            if let Some(s) = ua.get(&entry.node) {
                merged.extend(s.props.clone());
            }
            if let Some(s) = author.get(&entry.node) {
                merged.extend(s.props.clone());
            }

            // Пропускаем display:none.
            if merged.get("display").map(|v| v.as_str()) == Some("none") {
                return;
            }

            // Background-color.
            if let Some(bg) = merged.get("background-color") {
                if let Some(color) = parse_color(bg) {
                    let rect = Rect::from_xywh(x as f32, y as f32, w as f32, h as f32);
                    if let Some(rect) = rect {
                        pixmap.fill_rect(
                            rect,
                            &Paint {
                                shader: Shader::SolidColor(color),
                                ..Default::default()
                            },
                            Transform::identity(),
                            None,
                        );
                    }
                }
            }

            // Border (упрощённо — только rect outline одного цвета/толщины).
            if let Some(border) = merged.get("border") {
                if let Some((color, width)) = parse_border(border) {
                    let rect = Rect::from_xywh(x as f32, y as f32, w as f32, h as f32);
                    if let Some(rect) = rect {
                        let mut pb = PathBuilder::new();
                        pb.push_rect(rect);
                        let path = pb.finish().unwrap();
                        let stroke = Stroke {
                            width,
                            ..Default::default()
                        };
                        pixmap.stroke_path(
                            &path,
                            &Paint {
                                shader: Shader::SolidColor(color),
                                ..Default::default()
                            },
                            &stroke,
                            Transform::identity(),
                            None,
                        );
                    }
                }
            }
        }

        // Текст — отрисовать чёрными прямоугольниками как placeholder.
        if let Some(text) = &entry.text {
            // Высота строки ~ 0.7 * h или 14px, ширина ~ 0.6 * text.len() * font_size
            let font_size = 14.0_f32;
            let char_w = font_size * 0.6;
            let line_h = font_size * 1.2;
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                let mut cur_x = x;
                let mut cur_y = y;
                let max_x = x + w - char_w;
                let max_y = y + h - line_h;
                for ch in trimmed.chars() {
                    if cur_x > max_x {
                        cur_x = x;
                        cur_y += line_h;
                        if cur_y > max_y {
                            break;
                        }
                    }
                    if ch != ' ' {
                        let glyph = Rect::from_xywh(cur_x, cur_y, char_w * 0.8, font_size);
                        if let Some(glyph) = glyph {
                            pixmap.fill_rect(
                                glyph,
                                &Paint {
                                    shader: Shader::SolidColor(Color::BLACK),
                                    ..Default::default()
                                },
                                Transform::identity(),
                                None,
                            );
                        }
                    }
                    cur_x += char_w;
                }
            }
        }

        // Рекурсивно отрисовать детей.
        if let Ok(children) = tree.taffy.children(taffy_node) {
            for child in children {
                self.paint_node(doc, tree, ua, author, child, pixmap, x, y);
            }
        }
    }
}

/// Распарсить CSS color. Поддерживает #rgb, #rrggbb, named (black/white/red/...).
pub fn parse_color(s: &str) -> Option<Color> {
    let s = s.trim().to_lowercase();
    if let Some(hex) = s.strip_prefix('#') {
        let (r, g, b, a) = match hex.len() {
            3 => {
                let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()?;
                let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()?;
                let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()?;
                (r, g, b, 255)
            }
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                (r, g, b, 255)
            }
            8 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
                (r, g, b, a)
            }
            _ => return None,
        };
        return Some(Color::from_rgba8(r, g, b, a));
    }
    if s.starts_with("rgb(") {
        let inner = s.trim_start_matches("rgb(").trim_end_matches(')');
        let parts: Vec<&str> = inner.split(',').map(|p| p.trim()).collect();
        if parts.len() == 3 {
            let r: u8 = parts[0].parse().ok()?;
            let g: u8 = parts[1].parse().ok()?;
            let b: u8 = parts[2].parse().ok()?;
            return Some(Color::from_rgba8(r, g, b, 255));
        }
    }
    if s.starts_with("rgba(") {
        let inner = s.trim_start_matches("rgba(").trim_end_matches(')');
        let parts: Vec<&str> = inner.split(',').map(|p| p.trim()).collect();
        if parts.len() == 4 {
            let r: u8 = parts[0].parse().ok()?;
            let g: u8 = parts[1].parse().ok()?;
            let b: u8 = parts[2].parse().ok()?;
            let a_f: f32 = parts[3].parse().ok()?;
            let a = (a_f * 255.0) as u8;
            return Some(Color::from_rgba8(r, g, b, a));
        }
    }
    // Named colors
    let color = match s.as_str() {
        "black" => Color::BLACK,
        "white" => Color::WHITE,
        "red" => Color::from_rgba8(255, 0, 0, 255),
        "green" => Color::from_rgba8(0, 128, 0, 255),
        "blue" => Color::from_rgba8(0, 0, 255, 255),
        "yellow" => Color::from_rgba8(255, 255, 0, 255),
        "gray" | "grey" => Color::from_rgba8(128, 128, 128, 255),
        "silver" => Color::from_rgba8(192, 192, 192, 255),
        "maroon" => Color::from_rgba8(128, 0, 0, 255),
        "navy" => Color::from_rgba8(0, 0, 128, 255),
        "teal" => Color::from_rgba8(0, 128, 128, 255),
        "transparent" => Color::TRANSPARENT,
        _ => return None,
    };
    Some(color)
}

/// Распарсить border: "1px solid black" → (color, width).
fn parse_border(s: &str) -> Option<(Color, f32)> {
    let parts: Vec<&str> = s.split_whitespace().collect();
    let mut color = Color::BLACK;
    let mut width = 1.0;
    for p in parts {
        if let Some(n) = p.strip_suffix("px") {
            if let Ok(v) = n.parse::<f32>() {
                width = v;
            }
        } else if let Some(c) = parse_color(p) {
            color = c;
        }
    }
    Some((color, width))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paint_basic() {
        let html = r#"<html><body><p>Hello</p></body></html>"#;
        let doc = strata_html::parse_html(html).unwrap();
        let painter = Painter::new(800, 600);
        let result = painter.paint(&doc, None).unwrap();
        assert_eq!(result.width, 800);
        assert_eq!(result.height, 600);
        assert_eq!(result.data.len(), 800 * 600 * 4);
    }

    #[test]
    fn color_parsing() {
        assert_eq!(parse_color("black"), Some(Color::BLACK));
        assert_eq!(parse_color("white"), Some(Color::WHITE));
        assert_eq!(parse_color("#000"), Some(Color::BLACK));
        assert_eq!(parse_color("#ffffff"), Some(Color::WHITE));
        assert_eq!(parse_color("#ff0000"), Some(Color::from_rgba8(255, 0, 0, 255)));
        assert_eq!(parse_color("rgb(0, 0, 0)"), Some(Color::BLACK));
        assert_eq!(parse_color("rgba(0, 0, 0, 1)"), Some(Color::BLACK));
    }
}
