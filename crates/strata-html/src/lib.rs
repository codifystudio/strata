//! Strata HTML — ручной парсер HTML5.
//!
//! MVP: не реализует полную HTML5 spec (adoption agency algorithm и пр.),
//! но корректно обрабатывает 90% реальных страниц: теги, атрибуты,
//! текст, комментарии, doctype, void elements (img, br, hr, ...),
//! автоматически закрывает незакрытые теги.

use smol_str::SmolStr;
use strata_core::{StrataError, StrataResult};
use strata_dom::{Document, NodeId, NodeKind};

/// Void elements — не имеют закрывающего тега.
const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta",
    "param", "source", "track", "wbr",
];

/// Теги, внутри которых не нужно парсить HTML (содержимое — чистый текст).
const RAW_TEXT_ELEMENTS: &[&str] = &["script", "style", "textarea", "title"];

pub fn parse_html(input: &str) -> StrataResult<Document> {
    let mut p = Parser::new(input);
    let mut doc = Document::new();
    let root = doc.root();

    // Стек родителей для текущего открытого тега.
    let mut stack: Vec<NodeId> = vec![root];

    // html/head/body — добавим автоматически, если их нет в исходнике.
    // (упрощение: всегда добавляем, чтобы CSS с body/h1/etc работали)
    let html_id = doc.create_element("html", Vec::new());
    doc.append_child(root, html_id);
    stack.push(html_id);

    let head_id = doc.create_element("head", Vec::new());
    doc.append_child(html_id, head_id);

    let body_id = doc.create_element("body", Vec::new());
    doc.append_child(html_id, body_id);

    let mut in_head = false;
    let mut body_opened = false;

    while let Some(event) = p.next_event() {
        match event {
            Event::Doctype => {
                let id = doc.create_doctype();
                doc.append_child(*stack.last().unwrap(), id);
            }
            Event::Comment(text) => {
                let id = doc.create_comment(&text);
                if let Some(parent) = stack.last() {
                    doc.append_child(*parent, id);
                }
            }
            Event::Text(text) => {
                if text.trim().is_empty() {
                    continue;
                }
                let id = doc.create_text(&text);
                if let Some(parent) = stack.last() {
                    doc.append_child(*parent, id);
                }
            }
            Event::StartTag {
                tag,
                attrs,
                self_closing,
            } => {
                let tag_lower = tag.to_ascii_lowercase();
                let id = doc.create_element(&tag_lower, attrs);

                // Определить, в какого родителя append'ить.
                // Если html/head/body — не добавляем дубликат, используем существующий.
                if matches!(tag_lower.as_str(), "html" | "head" | "body") {
                    if tag_lower == "body" {
                        body_opened = true;
                        in_head = false;
                    } else if tag_lower == "head" {
                        in_head = true;
                    }
                    // не добавляем в стек, не создаём дубликат
                    continue;
                }

                // Обычный тег: parent = последний открытый тег в стеке.
                // Если стек пуст или содержит только root/html — добавить к body.
                let parent = {
                    let last = *stack.last().unwrap_or(&root);
                    let last_kind = doc.node(last).kind;
                    let is_container = matches!(
                        doc.node(last).element.as_ref().map(|e| e.tag.as_str()),
                        Some("html") | None
                    ) && last_kind == strata_dom::NodeKind::Element
                        || last_kind == strata_dom::NodeKind::Document;

                    if is_container && body_opened {
                        body_id
                    } else if is_container && in_head {
                        head_id
                    } else if is_container {
                        body_id
                    } else {
                        last
                    }
                };

                doc.append_child(parent, id);
                if !self_closing && !VOID_ELEMENTS.contains(&tag_lower.as_str()) {
                    stack.push(id);
                    // Для raw text elements — переключиться в режим чтения до close tag.
                    if RAW_TEXT_ELEMENTS.contains(&tag_lower.as_str()) {
                        let raw = p.consume_raw_text(&tag_lower);
                        if !raw.is_empty() {
                            let text_id = doc.create_text(&raw);
                            doc.append_child(id, text_id);
                        }
                        // consume_raw_text уже потребил </tag>.
                        stack.pop();
                    }
                }
            }
            Event::EndTag { tag } => {
                let tag_lower = tag.to_ascii_lowercase();
                if matches!(tag_lower.as_str(), "html" | "head" | "body") {
                    // не выходим за html_id
                    if tag_lower == "head" {
                        in_head = false;
                    }
                    continue;
                }
                // Найти ближайший открытый тег того же имени в стеке.
                let mut found_idx = None;
                for (i, open_id) in stack.iter().enumerate().rev() {
                    let node = doc.node(*open_id);
                    if let Some(e) = node.element.as_ref() {
                        if e.tag.eq_ignore_ascii_case(&tag_lower) {
                            found_idx = Some(i);
                            break;
                        }
                    }
                }
                if let Some(idx) = found_idx {
                    // Закрыть все теги до этого (включительно).
                    stack.truncate(idx);
                }
                // Если тег не найден — игнорируем (не закрываем ничего).
            }
        }
    }

    Ok(doc)
}

fn is_head_only(tag: &str) -> bool {
    matches!(
        tag,
        "title" | "meta" | "link" | "style" | "script" | "base" | "noscript"
    )
}

#[derive(Debug)]
enum Event {
    Doctype,
    Comment(String),
    Text(String),
    StartTag {
        tag: String,
        attrs: Vec<(SmolStr, SmolStr)>,
        self_closing: bool,
    },
    EndTag {
        tag: String,
    },
}

struct Parser<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, pos: 0 }
    }

    fn next_event(&mut self) -> Option<Event> {
        let bytes = self.input.as_bytes();
        if self.pos >= bytes.len() {
            return None;
        }

        // Текст до '<'.
        if bytes[self.pos] as char != '<' {
            let start = self.pos;
            while self.pos < bytes.len() && bytes[self.pos] as char != '<' {
                self.pos += 1;
            }
            let text = &self.input[start..self.pos];
            return Some(Event::Text(decode_entities(text)));
        }

        // У нас '<' — посмотри что дальше.
        if self.pos + 1 >= bytes.len() {
            self.pos = bytes.len();
            return Some(Event::Text(String::new()));
        }
        let next = bytes[self.pos + 1] as char;
        if next == '!' {
            // <!-- comment --> или <!doctype html>
            if self.input[self.pos..].starts_with("<!--") {
                // Найти конец '-->'.
                let end_marker = "-->";
                if let Some(end) = self.find_after(self.pos + 4, end_marker) {
                    let text = self.input[self.pos + 4..end].to_string();
                    self.pos = end + end_marker.len();
                    return Some(Event::Comment(text));
                } else {
                    let text = self.input[self.pos + 4..].to_string();
                    self.pos = bytes.len();
                    return Some(Event::Comment(text));
                }
            }
            // doctype or other declaration
            if let Some(end) = self.find_after(self.pos, ">") {
                let decl = &self.input[self.pos..end];
                self.pos = end + 1;
                if decl.to_ascii_lowercase().starts_with("<!doctype") {
                    return Some(Event::Doctype);
                }
                return self.next_event();
            }
            self.pos = bytes.len();
            return None;
        }
        if next == '/' {
            // Закрывающий тег.
            if let Some(end) = self.find_after(self.pos, ">") {
                let inner = &self.input[self.pos + 2..end].trim();
                let tag = inner.split_whitespace().next().unwrap_or("").to_string();
                self.pos = end + 1;
                if tag.is_empty() {
                    return self.next_event();
                }
                return Some(Event::EndTag { tag });
            }
            self.pos = bytes.len();
            return None;
        }
        if !next.is_ascii_alphabetic() {
            // Не тег — считать как текст.
            self.pos += 1;
            return Some(Event::Text("<".to_string()));
        }

        // Открывающий тег. Считать имя тега.
        let start = self.pos + 1;
        let mut i = start;
        while i < bytes.len() {
            let c = bytes[i] as char;
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                i += 1;
            } else {
                break;
            }
        }
        let tag = self.input[start..i].to_ascii_lowercase();
        self.pos = i;

        // Считать атрибуты.
        let mut attrs: Vec<(SmolStr, SmolStr)> = Vec::new();
        let mut self_closing = false;
        loop {
            self.skip_whitespace();
            if self.pos >= bytes.len() {
                break;
            }
            let c = bytes[self.pos] as char;
            if c == '>' {
                self.pos += 1;
                break;
            }
            if c == '/' {
                self_closing = true;
                self.pos += 1;
                // Пропустить возможный пробел и '>'.
                continue;
            }
            // Имя атрибута.
            let name_start = self.pos;
            while self.pos < bytes.len() {
                let cc = bytes[self.pos] as char;
                if cc.is_whitespace() || cc == '=' || cc == '>' || cc == '/' {
                    break;
                }
                self.pos += 1;
            }
            let name = self.input[name_start..self.pos].to_ascii_lowercase();
            if name.is_empty() {
                self.pos += 1; // скипаем один символ, чтобы не зациклиться
                continue;
            }

            self.skip_whitespace();
            let mut value = String::new();
            if self.pos < bytes.len() && bytes[self.pos] as char == '=' {
                self.pos += 1;
                self.skip_whitespace();
                if self.pos < bytes.len() {
                    let q = bytes[self.pos] as char;
                    if q == '"' || q == '\'' {
                        self.pos += 1;
                        let vstart = self.pos;
                        while self.pos < bytes.len() && bytes[self.pos] as char != q {
                            self.pos += 1;
                        }
                        value = decode_entities(&self.input[vstart..self.pos]);
                        if self.pos < bytes.len() {
                            self.pos += 1; // skip quote
                        }
                    } else {
                        // Unquoted value.
                        let vstart = self.pos;
                        while self.pos < bytes.len() {
                            let cc = bytes[self.pos] as char;
                            if cc.is_whitespace() || cc == '>' {
                                break;
                            }
                            self.pos += 1;
                        }
                        value = decode_entities(&self.input[vstart..self.pos]);
                    }
                }
            }
            attrs.push((SmolStr::new(name), SmolStr::new(value)));
        }

        Some(Event::StartTag {
            tag,
            attrs,
            self_closing,
        })
    }

    fn consume_raw_text(&mut self, tag: &str) -> String {
        let close_marker = format!("</{}", tag);
        let lower = self.input[self.pos..].to_ascii_lowercase();
        if let Some(relative) = lower.find(&close_marker) {
            let raw = self.input[self.pos..self.pos + relative].to_string();
            // Сдвинуть позицию за </tag>.
            if let Some(end) = self.find_after(self.pos + relative, ">") {
                self.pos = end + 1;
            } else {
                self.pos = self.input.len();
            }
            raw
        } else {
            let raw = self.input[self.pos..].to_string();
            self.pos = self.input.len();
            raw
        }
    }

    fn find_after(&self, from: usize, needle: &str) -> Option<usize> {
        self.input[from..].find(needle).map(|i| from + i)
    }

    fn skip_whitespace(&mut self) {
        let bytes = self.input.as_bytes();
        while self.pos < bytes.len() && (bytes[self.pos] as char).is_whitespace() {
            self.pos += 1;
        }
    }
}

fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] as char == '&' {
            if let Some(semicolon) = s[i..].find(';') {
                let entity = &s[i + 1..i + semicolon];
                if let Some(decoded) = decode_entity(entity) {
                    out.push_str(&decoded);
                    i += semicolon + 1;
                    continue;
                }
            }
            out.push('&');
            i += 1;
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

fn decode_entity(entity: &str) -> Option<String> {
    if let Some(hex) = entity.strip_prefix("#x").or_else(|| entity.strip_prefix("#X")) {
        let n = u32::from_str_radix(hex, 16).ok()?;
        return char::from_u32(n).map(|c| c.to_string());
    }
    if let Some(dec) = entity.strip_prefix('#') {
        let n: u32 = dec.parse().ok()?;
        return char::from_u32(n).map(|c| c.to_string());
    }
    let s = match entity {
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "quot" => "\"",
        "apos" => "'",
        "nbsp" => "\u{00A0}",
        "copy" => "\u{00A9}",
        "reg" => "\u{00AE}",
        "trade" => "\u{2122}",
        "mdash" => "\u{2014}",
        "ndash" => "\u{2013}",
        "hellip" => "\u{2026}",
        "laquo" => "\u{00AB}",
        "raquo" => "\u{00BB}",
        "ldquo" => "\u{201C}",
        "rdquo" => "\u{201D}",
        "lsquo" => "\u{2018}",
        "rsquo" => "\u{2019}",
        _ => return None,
    };
    Some(s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_basic_html() {
        let html = r#"<!DOCTYPE html>
<html>
<head><title>Test</title></head>
<body>
  <h1 id="hi" class="big red">Hello</h1>
  <p>World</p>
</body>
</html>"#;

        let doc = parse_html(html).unwrap();

        let h1s = doc.find_by_tag("h1");
        assert_eq!(h1s.len(), 1);
        let h1 = h1s[0];
        let elem = doc.node(h1).element.as_ref().unwrap();
        assert_eq!(elem.id.as_deref(), Some("hi"));
        assert!(elem.classes.iter().any(|c| c == "big"));
        assert!(elem.classes.iter().any(|c| c == "red"));
        assert_eq!(doc.text_content(h1), "Hello");

        let ps = doc.find_by_tag("p");
        assert_eq!(ps.len(), 1);
        assert_eq!(doc.text_content(ps[0]), "World");
    }

    #[test]
    fn parse_text_node_merge() {
        let html = "<p>Hello <b>bold</b> world</p>";
        let doc = parse_html(html).unwrap();
        let ps = doc.find_by_tag("p");
        assert_eq!(ps.len(), 1);
        assert_eq!(doc.text_content(ps[0]), "Hello bold world");
    }

    #[test]
    fn parse_void_elements() {
        let html = r#"<p>Line<br>break<hr>end</p>"#;
        let doc = parse_html(html).unwrap();
        let ps = doc.find_by_tag("p");
        assert_eq!(ps.len(), 1);
        let p = ps[0];
        // Должен содержать text "Line", br, text "break", hr, text "end"
        assert_eq!(doc.text_content(p), "Linebreakend");
    }

    #[test]
    fn parse_entities() {
        let html = r#"<p>AT&amp;T &lt;tag&gt; &quot;quoted&quot;</p>"#;
        let doc = parse_html(html).unwrap();
        let p = doc.find_by_tag("p")[0];
        assert_eq!(doc.text_content(p), r#"AT&T <tag> "quoted""#);
    }

    #[test]
    fn parse_raw_text_script() {
        let html = r#"<script>if (a < b) { console.log("<div>"); }</script>"#;
        let doc = parse_html(html).unwrap();
        let scripts = doc.find_by_tag("script");
        assert_eq!(scripts.len(), 1);
        let script = scripts[0];
        let text = doc.text_content(script);
        assert!(text.contains("if (a < b)"));
        assert!(text.contains("console.log"));
    }
}
