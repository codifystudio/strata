//! Strata layout — построение layout tree из DOM + стилей и расчёт через taffy.
//!
//! MVP: поддерживает block, inline, flex (display:flex), базовые размеры
//! (width/height в px / %, padding, margin, border).

use ahash::AHashMap;
use strata_core::{StrataError, StrataResult};
use strata_css::{DeclaredStyle, StyleSheet, UA_STYLESHEET};
use strata_dom::{Document, NodeId, NodeKind};
use taffy::prelude::*;
use taffy::{
    geometry::Size, AvailableSpace, Dimension, Layout, NodeId as TaffyNodeId, Rect, Style,
    TaffyTree,
};

/// Один элемент в layout tree с привязкой к NodeId.
pub struct LayoutEntry {
    pub node: NodeId,
    pub taffy_node: TaffyNodeId,
    pub text: Option<String>,
}

pub struct LayoutTree {
    pub taffy: TaffyTree,
    pub entries: Vec<LayoutEntry>,
    pub root: TaffyNodeId,
    /// Карта для быстрого поиска entry по NodeId.
    pub by_node: AHashMap<NodeId, TaffyNodeId>,
}

impl LayoutTree {
    pub fn build(
        doc: &Document,
        author_sheet: Option<&StyleSheet>,
        viewport: Size<u32>,
    ) -> StrataResult<Self> {
        let mut taffy = TaffyTree::new();
        let mut by_node: AHashMap<NodeId, TaffyNodeId> = AHashMap::new();
        let mut entries: Vec<LayoutEntry> = Vec::new();

        let ua = StyleSheet::parse(UA_STYLESHEET)
            .map_err(|e| StrataError::Layout(format!("UA parse: {}", e)))?;
        let ua_styles = ua.compute(doc);
        let author_styles = author_sheet.map(|s| s.compute(doc)).unwrap_or_default();

        let root_node_id = doc.root();
        let root_taffy = build_node(
            doc,
            &ua_styles,
            &author_styles,
            root_node_id,
            &mut taffy,
            &mut by_node,
            &mut entries,
        )?;

        let vp_w = viewport.width as f32;
        let vp_h = viewport.height as f32;
        let root_style = Style {
            size: Size {
                width: Dimension::Length(vp_w),
                height: Dimension::Length(vp_h),
            },
            ..Default::default()
        };
        let _ = taffy.set_style(root_taffy, root_style);

        taffy
            .compute_layout(
                root_taffy,
                Size {
                    width: AvailableSpace::Definite(vp_w),
                    height: AvailableSpace::Definite(vp_h),
                },
            )
            .map_err(|e: taffy::TaffyError| StrataError::Layout(e.to_string()))?;

        Ok(Self {
            taffy,
            entries,
            root: root_taffy,
            by_node,
        })
    }

    pub fn layout(&self, node: NodeId) -> Option<&Layout> {
        let taffy_node = self.by_node.get(&node)?;
        self.taffy.layout(*taffy_node).ok()
    }
}

fn build_node(
    doc: &Document,
    ua: &AHashMap<NodeId, DeclaredStyle>,
    author: &AHashMap<NodeId, DeclaredStyle>,
    node_id: NodeId,
    taffy: &mut TaffyTree,
    by_node: &mut AHashMap<NodeId, TaffyNodeId>,
    entries: &mut Vec<LayoutEntry>,
) -> StrataResult<TaffyNodeId> {
    let node = doc.node(node_id);

    match node.kind {
        NodeKind::Element | NodeKind::Document => {
            let mut merged: AHashMap<String, String> = AHashMap::new();
            if let Some(s) = ua.get(&node_id) {
                merged.extend(s.props.clone());
            }
            if let Some(s) = author.get(&node_id) {
                merged.extend(s.props.clone());
            }

            let style = compute_taffy_style(&merged);
            let taffy_node = taffy
                .new_leaf(style)
                .map_err(|e| StrataError::Layout(e.to_string()))?;
            by_node.insert(node_id, taffy_node);
            entries.push(LayoutEntry {
                node: node_id,
                taffy_node,
                text: None,
            });

            let mut child_taffys: Vec<TaffyNodeId> = Vec::new();
            for child in node.children.clone() {
                match doc.node(child).kind {
                    NodeKind::Element | NodeKind::Text => {}
                    _ => continue,
                }
                if doc.node(child).kind == NodeKind::Text {
                    let text = doc.node(child).text.as_deref().unwrap_or("").to_string();
                    if text.trim().is_empty() {
                        continue;
                    }
                    let text_taffy = taffy
                        .new_leaf(Style::default())
                        .map_err(|e| StrataError::Layout(e.to_string()))?;
                    by_node.insert(child, text_taffy);
                    entries.push(LayoutEntry {
                        node: child,
                        taffy_node: text_taffy,
                        text: Some(text),
                    });
                    child_taffys.push(text_taffy);
                } else {
                    let child_taffy = build_node(doc, ua, author, child, taffy, by_node, entries)?;
                    child_taffys.push(child_taffy);
                }
            }
            taffy
                .set_children(taffy_node, &child_taffys)
                .map_err(|e| StrataError::Layout(e.to_string()))?;

            Ok(taffy_node)
        }
        _ => Err(StrataError::Layout(format!(
            "node {:?} not renderable",
            node_id
        ))),
    }
}

fn compute_taffy_style(declared: &AHashMap<String, String>) -> Style {
    let mut s = Style::default();

    if let Some(d) = declared.get("display") {
        match d.as_str() {
            "block" => s.display = taffy::Display::Block,
            "flex" => s.display = taffy::Display::Flex,
            "none" => s.display = taffy::Display::None,
            _ => {}
        }
    }

    if let Some(v) = declared.get("flex-direction") {
        s.flex_direction = match v.as_str() {
            "row" => taffy::FlexDirection::Row,
            "row-reverse" => taffy::FlexDirection::RowReverse,
            "column" => taffy::FlexDirection::Column,
            "column-reverse" => taffy::FlexDirection::ColumnReverse,
            _ => taffy::FlexDirection::Row,
        };
    }

    if let Some(v) = declared.get("flex-wrap") {
        s.flex_wrap = match v.as_str() {
            "wrap" => taffy::FlexWrap::Wrap,
            "wrap-reverse" => taffy::FlexWrap::WrapReverse,
            _ => taffy::FlexWrap::NoWrap,
        };
    }

    if let Some(v) = declared.get("justify-content") {
        s.justify_content = Some(match v.as_str() {
            "flex-start" => taffy::JustifyContent::FlexStart,
            "flex-end" => taffy::JustifyContent::FlexEnd,
            "center" => taffy::JustifyContent::Center,
            "space-between" => taffy::JustifyContent::SpaceBetween,
            "space-around" => taffy::JustifyContent::SpaceAround,
            "space-evenly" => taffy::JustifyContent::SpaceEvenly,
            _ => taffy::JustifyContent::FlexStart,
        });
    }

    if let Some(v) = declared.get("align-items") {
        s.align_items = Some(match v.as_str() {
            "flex-start" => taffy::AlignItems::FlexStart,
            "flex-end" => taffy::AlignItems::FlexEnd,
            "center" => taffy::AlignItems::Center,
            "stretch" => taffy::AlignItems::Stretch,
            _ => taffy::AlignItems::FlexStart,
        });
    }

    if let Some(v) = declared.get("width") {
        s.size.width = parse_dimension(v);
    }
    if let Some(v) = declared.get("height") {
        s.size.height = parse_dimension(v);
    }
    if let Some(v) = declared.get("min-width") {
        s.min_size.width = parse_dimension(v);
    }
    if let Some(v) = declared.get("min-height") {
        s.min_size.height = parse_dimension(v);
    }
    if let Some(v) = declared.get("max-width") {
        s.max_size.width = parse_dimension(v);
    }
    if let Some(v) = declared.get("max-height") {
        s.max_size.height = parse_dimension(v);
    }

    if let Some(v) = declared.get("padding") {
        s.padding = parse_rect_lp(v);
    }
    if let Some(v) = declared.get("margin") {
        s.margin = parse_rect_lpa(v);
    }
    if let Some(v) = declared.get("border-width") {
        s.border = parse_rect_lp(v);
    } else if declared.contains_key("border") {
        // border: 1px solid black → все 4 стороны по 1px
        if let Some(width) = declared.get("border").and_then(|s| {
            s.split_whitespace()
                .find_map(|p| p.strip_suffix("px").and_then(|n| n.parse::<f32>().ok()))
        }) {
            s.border = Rect {
                top: taffy::LengthPercentage::Length(width),
                right: taffy::LengthPercentage::Length(width),
                bottom: taffy::LengthPercentage::Length(width),
                left: taffy::LengthPercentage::Length(width),
            };
        }
    }

    s
}

fn parse_dimension(v: &str) -> Dimension {
    let v = v.trim();
    if v == "auto" {
        return Dimension::Auto;
    }
    if let Some(n) = parse_px(v) {
        return Dimension::Length(n);
    }
    if v.ends_with('%') {
        return Dimension::Percent(v.trim_end_matches('%').parse::<f32>().unwrap_or(0.0) / 100.0);
    }
    Dimension::Auto
}

fn parse_px(v: &str) -> Option<f32> {
    let v = v.trim().trim_end_matches("px").trim();
    v.parse::<f32>().ok()
}

fn parse_rect_lp(v: &str) -> Rect<taffy::LengthPercentage> {
    let parts: Vec<&str> = v.split_whitespace().collect();
    let to_lp = |s: &str| -> taffy::LengthPercentage {
        if let Some(n) = parse_px(s) {
            taffy::LengthPercentage::Length(n)
        } else if s.ends_with('%') {
            let p = s.trim_end_matches('%').parse::<f32>().unwrap_or(0.0) / 100.0;
            taffy::LengthPercentage::Percent(p)
        } else {
            taffy::LengthPercentage::Length(0.0)
        }
    };
    match parts.len() {
        1 => Rect {
            left: to_lp(parts[0]),
            right: to_lp(parts[0]),
            top: to_lp(parts[0]),
            bottom: to_lp(parts[0]),
        },
        2 => Rect {
            top: to_lp(parts[0]),
            right: to_lp(parts[1]),
            bottom: to_lp(parts[0]),
            left: to_lp(parts[1]),
        },
        3 => Rect {
            top: to_lp(parts[0]),
            right: to_lp(parts[1]),
            bottom: to_lp(parts[2]),
            left: to_lp(parts[1]),
        },
        4 => Rect {
            top: to_lp(parts[0]),
            right: to_lp(parts[1]),
            bottom: to_lp(parts[2]),
            left: to_lp(parts[3]),
        },
        _ => Rect::length(0.0),
    }
}

fn parse_rect_lpa(v: &str) -> Rect<taffy::LengthPercentageAuto> {
    let parts: Vec<&str> = v.split_whitespace().collect();
    let to_lpa = |s: &str| -> taffy::LengthPercentageAuto {
        if s == "auto" {
            return taffy::LengthPercentageAuto::Auto;
        }
        if let Some(n) = parse_px(s) {
            return taffy::LengthPercentageAuto::Length(n);
        }
        if s.ends_with('%') {
            let p = s.trim_end_matches('%').parse::<f32>().unwrap_or(0.0) / 100.0;
            return taffy::LengthPercentageAuto::Percent(p);
        }
        taffy::LengthPercentageAuto::Length(0.0)
    };
    match parts.len() {
        1 => Rect {
            left: to_lpa(parts[0]),
            right: to_lpa(parts[0]),
            top: to_lpa(parts[0]),
            bottom: to_lpa(parts[0]),
        },
        2 => Rect {
            top: to_lpa(parts[0]),
            right: to_lpa(parts[1]),
            bottom: to_lpa(parts[0]),
            left: to_lpa(parts[1]),
        },
        3 => Rect {
            top: to_lpa(parts[0]),
            right: to_lpa(parts[1]),
            bottom: to_lpa(parts[2]),
            left: to_lpa(parts[1]),
        },
        4 => Rect {
            top: to_lpa(parts[0]),
            right: to_lpa(parts[1]),
            bottom: to_lpa(parts[2]),
            left: to_lpa(parts[3]),
        },
        _ => Rect::auto(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_simple_tree() {
        let html = r#"<html><body><p>Hello</p><p>World</p></body></html>"#;
        let doc = strata_html::parse_html(html).unwrap();
        let sheet = StyleSheet::parse(UA_STYLESHEET).unwrap();
        let tree = LayoutTree::build(
            &doc,
            Some(&sheet),
            Size { width: 800, height: 600 },
        )
        .unwrap();
        let body = doc.find_by_tag("body")[0];
        let layout = tree.layout(body).expect("body should have layout");
        // taffy block layout без явной ширины может дать 0 — но layout должен быть.
        let _ = layout;
    }
}
