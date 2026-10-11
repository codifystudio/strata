//! Strata CSS — парсер CSS и matcher селекторов.
//!
//! MVP: ручной парсер без зависимостей. Поддерживает:
//! - Селекторы: tag, .class, #id, *, descendant ( ), child (>),
//!   next sibling (+), subsequent sibling (~), запятая.
//! - Свойства: `name: value;` — значение собирается до `;` или `}`.
//! - Комментарии `/* ... */` и `//` (последние пропускаются до конца строки).
//! - @-правила: пропускаются целиком (с блоком или без).
//!
//! Не поддерживает: псевдоклассы (кроме простых), @media queries, @keyframes,
//! CSS variables, calc(), вложенные правила.

use ahash::AHashMap;
use smol_str::SmolStr;
use strata_core::{StrataError, StrataResult};
use strata_dom::{Document, ElementData, NodeId, NodeKind};

// ─── Selector ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SimpleSelector {
    Universal,
    Tag(SmolStr),
    Class(SmolStr),
    Id(SmolStr),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompoundSelector {
    pub parts: Vec<SimpleSelector>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Combinator {
    Descendant,
    Child,
    NextSibling,
    SubsequentSibling,
}

#[derive(Debug, Clone)]
pub struct ComplexSelector {
    pub compounds: Vec<CompoundSelector>,
    pub combinators: Vec<Combinator>,
}

#[derive(Debug, Clone)]
pub struct Selector {
    pub complex: ComplexSelector,
    pub specificity: u32,
}

impl Selector {
    pub fn compute_specificity(c: &ComplexSelector) -> u32 {
        let mut a = 0u32;
        let mut b = 0u32;
        let mut cc = 0u32;
        for comp in &c.compounds {
            for part in &comp.parts {
                match part {
                    SimpleSelector::Id(_) => a += 1,
                    SimpleSelector::Class(_) => b += 1,
                    SimpleSelector::Tag(_) => cc += 1,
                    SimpleSelector::Universal => {}
                }
            }
        }
        (a << 16) | (b << 8) | cc
    }

    pub fn matches(&self, doc: &Document, node: NodeId) -> bool {
        if doc.node(node).kind != NodeKind::Element {
            return false;
        }
        let compounds = &self.complex.compounds;
        let combinators = &self.complex.combinators;
        let last = compounds.len();
        if last == 0 {
            return false;
        }
        if !compound_matches(doc, node, &compounds[last - 1]) {
            return false;
        }
        match_compounds(doc, node, compounds, combinators, last - 1)
    }
}

fn match_compounds(
    doc: &Document,
    node: NodeId,
    compounds: &[CompoundSelector],
    combinators: &[Combinator],
    idx: usize,
) -> bool {
    if idx == 0 {
        return true;
    }
    let combinator = combinators[idx - 1];
    let ancestor_compound = &compounds[idx - 1];

    match combinator {
        Combinator::Descendant => {
            let mut cur = doc.node(node).parent;
            while let Some(p) = cur {
                if doc.node(p).kind == NodeKind::Element
                    && compound_matches(doc, p, ancestor_compound)
                    && match_compounds(doc, p, compounds, combinators, idx - 1)
                {
                    return true;
                }
                cur = doc.node(p).parent;
            }
            false
        }
        Combinator::Child => {
            if let Some(p) = doc.node(node).parent {
                if doc.node(p).kind == NodeKind::Element
                    && compound_matches(doc, p, ancestor_compound)
                    && match_compounds(doc, p, compounds, combinators, idx - 1)
                {
                    return true;
                }
            }
            false
        }
        Combinator::NextSibling => {
            if let Some(prev) = prev_sibling_element(doc, node) {
                if compound_matches(doc, prev, ancestor_compound)
                    && match_compounds(doc, prev, compounds, combinators, idx - 1)
                {
                    return true;
                }
            }
            false
        }
        Combinator::SubsequentSibling => {
            let mut cur = prev_sibling_element(doc, node);
            while let Some(prev) = cur {
                if compound_matches(doc, prev, ancestor_compound)
                    && match_compounds(doc, prev, compounds, combinators, idx - 1)
                {
                    return true;
                }
                cur = prev_sibling_element(doc, prev);
            }
            false
        }
    }
}

fn prev_sibling_element(doc: &Document, node: NodeId) -> Option<NodeId> {
    let parent = doc.node(node).parent?;
    let children = doc.node(parent).children.clone();
    let idx = children.iter().position(|c| *c == node)?;
    for prev in children[..idx].iter().rev() {
        if doc.node(*prev).kind == NodeKind::Element {
            return Some(*prev);
        }
    }
    None
}

fn compound_matches(doc: &Document, node: NodeId, compound: &CompoundSelector) -> bool {
    let element = match doc.node(node).element.as_ref() {
        Some(e) => e,
        None => return false,
    };
    for part in &compound.parts {
        if !simple_matches(element, part) {
            return false;
        }
    }
    true
}

fn simple_matches(e: &ElementData, s: &SimpleSelector) -> bool {
    match s {
        SimpleSelector::Universal => true,
        SimpleSelector::Tag(t) => e.tag.eq_ignore_ascii_case(t),
        SimpleSelector::Class(c) => e.classes.iter().any(|x| x.eq_ignore_ascii_case(c)),
        SimpleSelector::Id(id) => e
            .id
            .as_ref()
            .map(|x| x.eq_ignore_ascii_case(id))
            .unwrap_or(false),
    }
}

// ─── Selector parser ───────────────────────────────────────────────────────

pub fn parse_selectors(input: &str) -> StrataResult<Vec<Selector>> {
    let mut out = Vec::new();
    for part in input.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let c = parse_complex(part)?;
        out.push(Selector {
            specificity: Selector::compute_specificity(&c),
            complex: c,
        });
    }
    Ok(out)
}

fn parse_complex(input: &str) -> StrataResult<ComplexSelector> {
    let bytes = input.as_bytes();
    let mut compounds: Vec<CompoundSelector> = Vec::new();
    let mut combinators: Vec<Combinator> = Vec::new();
    let mut current: Vec<SimpleSelector> = Vec::new();
    let mut pending_combinator: Option<Combinator> = None;
    let mut last_was_space = false;
    let mut i = 0;

    while i < bytes.len() {
        let c = bytes[i] as char;
        match c {
            ' ' | '\t' | '\n' | '\r' => {
                if !current.is_empty() {
                    last_was_space = true;
                }
                i += 1;
            }
            '>' => {
                if !current.is_empty() {
                    compounds.push(CompoundSelector { parts: std::mem::take(&mut current) });
                    pending_combinator = Some(Combinator::Child);
                    last_was_space = false;
                }
                i += 1;
            }
            '+' => {
                if !current.is_empty() {
                    compounds.push(CompoundSelector { parts: std::mem::take(&mut current) });
                    pending_combinator = Some(Combinator::NextSibling);
                    last_was_space = false;
                }
                i += 1;
            }
            '~' => {
                if !current.is_empty() {
                    compounds.push(CompoundSelector { parts: std::mem::take(&mut current) });
                    pending_combinator = Some(Combinator::SubsequentSibling);
                    last_was_space = false;
                }
                i += 1;
            }
            '*' => {
                current.push(SimpleSelector::Universal);
                last_was_space = false;
                i += 1;
            }
            '#' => {
                i += 1;
                let start = i;
                while i < bytes.len() && is_ident_char(bytes[i] as char) {
                    i += 1;
                }
                let name = std::str::from_utf8(&bytes[start..i]).unwrap_or("");
                current.push(SimpleSelector::Id(SmolStr::new(name)));
                last_was_space = false;
            }
            '.' => {
                i += 1;
                let start = i;
                while i < bytes.len() && is_ident_char(bytes[i] as char) {
                    i += 1;
                }
                let name = std::str::from_utf8(&bytes[start..i]).unwrap_or("");
                current.push(SimpleSelector::Class(SmolStr::new(name)));
                last_was_space = false;
            }
            _ if is_ident_start(c) => {
                if last_was_space && !current.is_empty() {
                    compounds.push(CompoundSelector { parts: std::mem::take(&mut current) });
                    pending_combinator = Some(Combinator::Descendant);
                    last_was_space = false;
                }
                if let Some(com) = pending_combinator.take() {
                    combinators.push(com);
                }
                let start = i;
                while i < bytes.len() && is_ident_char(bytes[i] as char) {
                    i += 1;
                }
                let name = std::str::from_utf8(&bytes[start..i]).unwrap_or("");
                current.push(SimpleSelector::Tag(SmolStr::new(name)));
            }
            _ => {
                // Пропускаем неизвестные символы (атрибуты [..] и пр.)
                i += 1;
            }
        }
    }
    if !current.is_empty() {
        compounds.push(CompoundSelector { parts: current });
    }
    if compounds.is_empty() {
        return Err(StrataError::CssParse(format!("empty selector: {}", input)));
    }
    while combinators.len() + 1 < compounds.len() {
        combinators.push(Combinator::Descendant);
    }
    Ok(ComplexSelector { compounds, combinators })
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || c == '-'
}

fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

// ─── Declared style ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct DeclaredStyle {
    pub props: AHashMap<String, String>,
}

// ─── Rule / StyleSheet ────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct StyleRule {
    pub selectors: Vec<Selector>,
    pub style: DeclaredStyle,
    pub specificity: u32,
}

#[derive(Debug, Default)]
pub struct StyleSheet {
    pub rules: Vec<StyleRule>,
}

impl StyleSheet {
    pub fn new() -> Self {
        Self::default()
    }

    /// Распарсить CSS. Поддерживаются только rule-list (без @media и т.д.).
    pub fn parse(css: &str) -> StrataResult<Self> {
        let mut out = Self::new();
        let bytes = css.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            // Пропустить whitespace и комментарии.
            i = skip_whitespace_and_comments(css, i);
            if i >= bytes.len() {
                break;
            }

            // @-правило — пропустить целиком.
            if bytes[i] as char == '@' {
                i = skip_at_rule(css, i);
                continue;
            }

            // Считать prelude до '{'.
            let prelude_start = i;
            while i < bytes.len() && bytes[i] as char != '{' {
                // Пропустить возможные комментарии внутри prelude.
                if bytes[i] as char == '/' && i + 1 < bytes.len() && bytes[i + 1] as char == '*' {
                    i = skip_block_comment(css, i);
                    continue;
                }
                i += 1;
            }
            if i >= bytes.len() {
                break;
            }
            let prelude = &css[prelude_start..i];
            i += 1; // skip '{'

            // Считать тело до '}'.
            let body_start = i;
            let mut depth = 1;
            while i < bytes.len() && depth > 0 {
                let c = bytes[i] as char;
                if c == '{' {
                    depth += 1;
                } else if c == '}' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                } else if c == '/' && i + 1 < bytes.len() && bytes[i + 1] as char == '*' {
                    i = skip_block_comment(css, i);
                    continue;
                }
                i += 1;
            }
            let body = &css[body_start..i];
            if i < bytes.len() {
                i += 1; // skip '}'
            }

            // Распарсить селекторы.
            let selectors = match parse_selectors(prelude) {
                Ok(s) => s,
                Err(e) => {
                    log::warn!("css: bad selector {:?}: {}", prelude.trim(), e);
                    continue;
                }
            };
            if selectors.is_empty() {
                continue;
            }

            // Распарсить тело как декларации.
            let style = parse_declarations(body);
            let specificity = selectors.iter().map(|s| s.specificity).min().unwrap_or(0);

            out.rules.push(StyleRule {
                selectors,
                specificity,
                style,
            });
        }
        Ok(out)
    }

    /// Применить стили ко всем элементам документа.
    pub fn compute(&self, doc: &Document) -> AHashMap<NodeId, DeclaredStyle> {
        let mut out: AHashMap<NodeId, DeclaredStyle> = AHashMap::new();

        let mut rules: Vec<&StyleRule> = self.rules.iter().collect();
        rules.sort_by_key(|r| r.specificity);

        let nodes = collect_all_elements(doc, doc.root());
        for node_id in nodes {
            for rule in &rules {
                let matched = rule.selectors.iter().any(|s| s.matches(doc, node_id));
                if matched {
                    let entry = out.entry(node_id).or_default();
                    for (k, v) in &rule.style.props {
                        entry.props.insert(k.clone(), v.clone());
                    }
                }
            }
        }

        out
    }
}

fn parse_declarations(body: &str) -> DeclaredStyle {
    let mut props: AHashMap<String, String> = AHashMap::new();
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        i = skip_whitespace_and_comments(body, i);
        if i >= bytes.len() {
            break;
        }
        // property name
        let name_start = i;
        while i < bytes.len() {
            let c = bytes[i] as char;
            if c == ':' || c == ';' || c == '}' || c.is_whitespace() {
                break;
            }
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let name = body[name_start..i].trim().to_lowercase();
        // skip whitespace
        while i < bytes.len() && (bytes[i] as char).is_whitespace() {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] as char != ':' {
            // Не декларация — пропустить до ;
            while i < bytes.len() && bytes[i] as char != ';' {
                i += 1;
            }
            if i < bytes.len() {
                i += 1;
            }
            continue;
        }
        i += 1; // skip ':'
        let value_start = i;
        while i < bytes.len() && bytes[i] as char != ';' && bytes[i] as char != '}' {
            // skip comments
            if bytes[i] as char == '/' && i + 1 < bytes.len() && bytes[i + 1] as char == '*' {
                i = skip_block_comment(body, i);
                continue;
            }
            i += 1;
        }
        let value = body[value_start..i].trim().to_string();
        if !name.is_empty() && !value.is_empty() {
            props.insert(name, value);
        }
        if i < bytes.len() && bytes[i] as char == ';' {
            i += 1;
        }
    }
    DeclaredStyle { props }
}

fn skip_whitespace_and_comments(s: &str, mut i: usize) -> usize {
    let bytes = s.as_bytes();
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '/' && i + 1 < bytes.len() {
            let next = bytes[i + 1] as char;
            if next == '*' {
                i = skip_block_comment(s, i);
                continue;
            }
            if next == '/' {
                // line comment — до конца строки
                i += 2;
                while i < bytes.len() && bytes[i] as char != '\n' {
                    i += 1;
                }
                continue;
            }
        }
        break;
    }
    i
}

fn skip_block_comment(s: &str, mut i: usize) -> usize {
    let bytes = s.as_bytes();
    if !(bytes[i] as char == '/' && i + 1 < bytes.len() && bytes[i + 1] as char == '*') {
        return i;
    }
    i += 2;
    while i + 1 < bytes.len() && !(bytes[i] as char == '*' && bytes[i + 1] as char == '/') {
        i += 1;
    }
    if i + 1 < bytes.len() {
        i += 2;
    } else {
        i = bytes.len();
    }
    i
}

/// Пропустить @-правило целиком (с блоком или без).
fn skip_at_rule(s: &str, mut i: usize) -> usize {
    let bytes = s.as_bytes();
    // @name до { или ;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c == '{' {
            // пропустить блок с учётом вложенности
            let mut depth = 1;
            i += 1;
            while i < bytes.len() && depth > 0 {
                let cc = bytes[i] as char;
                if cc == '{' {
                    depth += 1;
                } else if cc == '}' {
                    depth -= 1;
                }
                i += 1;
            }
            return i;
        }
        if c == ';' {
            return i + 1;
        }
        // комментарии внутри @-правила
        if c == '/' && i + 1 < bytes.len() && bytes[i + 1] as char == '*' {
            i = skip_block_comment(s, i);
            continue;
        }
        i += 1;
    }
    i
}

fn collect_all_elements(doc: &Document, root: NodeId) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        let node = doc.node(n);
        if node.kind == NodeKind::Element {
            out.push(n);
        }
        for c in node.children.iter().rev() {
            stack.push(*c);
        }
    }
    out
}

/// User agent stylesheet (минимальный reset для читаемости).
pub const UA_STYLESHEET: &str = r#"
html { display: block; }
head, title, meta, link, style, script { display: none; }
body { display: block; margin: 8px; font-family: sans-serif; font-size: 16px; color: black; background-color: white; }
p, div, section, article, header, footer, main, nav, aside, h1, h2, h3, h4, h5, h6,
ul, ol, li, table, tr, td, th, tbody, thead, tfoot, figure, figcaption, blockquote,
pre, hr, form, fieldset, address { display: block; }
span, a, strong, em, b, i, code, small, big, sub, sup, mark, abbr, cite,
q, var, kbd, samp, time, u, s, del, ins, br, img, input, button, label,
select, option, textarea { display: inline; }
h1 { font-size: 2em; font-weight: bold; margin: 0.67em 0; }
h2 { font-size: 1.5em; font-weight: bold; margin: 0.83em 0; }
h3 { font-size: 1.17em; font-weight: bold; margin: 1em 0; }
h4, h5, h6 { font-weight: bold; margin: 1.33em 0; }
p { margin: 1em 0; }
a { color: blue; text-decoration: underline; }
b, strong { font-weight: bold; }
i, em { font-style: italic; }
ul, ol { margin: 1em 0; padding-left: 40px; }
li { display: list-item; }
img { display: inline-block; }
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_css() {
        let css = r#"
            body { color: red; background-color: white; }
            .foo { font-size: 14px; }
            #bar { display: none; }
            div p > a { color: green; }
        "#;
        let sheet = StyleSheet::parse(css).unwrap();
        assert_eq!(sheet.rules.len(), 4);
    }

    #[test]
    fn compute_styles() {
        let html = r#"<html><body><p class="text">Hi</p></body></html>"#;
        let doc = strata_html::parse_html(html).unwrap();
        let css = "p { color: red; } .text { font-size: 14px; }";
        let sheet = StyleSheet::parse(css).unwrap();
        let styles = sheet.compute(&doc);

        let p = doc.find_by_tag("p")[0];
        let style = styles.get(&p).unwrap();
        assert_eq!(style.props.get("color"), Some(&"red".to_string()));
        assert_eq!(style.props.get("font-size"), Some(&"14px".to_string()));
    }

    #[test]
    fn descendant_selector() {
        let html = r#"<html><body><div><p class="inner">hi</p></div></body></html>"#;
        let doc = strata_html::parse_html(html).unwrap();
        let css = "div p { color: green; }";
        let sheet = StyleSheet::parse(css).unwrap();
        let styles = sheet.compute(&doc);
        let p = doc.find_by_tag("p")[0];
        let style = styles.get(&p).unwrap();
        assert_eq!(style.props.get("color"), Some(&"green".to_string()));
    }

    #[test]
    fn child_selector_not_matching() {
        let html = r#"<html><body><div><span><p>x</p></span></div></body></html>"#;
        let doc = strata_html::parse_html(html).unwrap();
        let css = "div > p { color: red; }";
        let sheet = StyleSheet::parse(css).unwrap();
        let styles = sheet.compute(&doc);
        let p = doc.find_by_tag("p")[0];
        assert!(
            styles.get(&p).is_none()
                || styles.get(&p).and_then(|s| s.props.get("color")).is_none()
        );
    }

    #[test]
    fn id_and_class() {
        let html = r##"<html><body><a id="link" class="nav-item primary" href="#">x</a></body></html>"##;
        let doc = strata_html::parse_html(html).unwrap();
        let css = "a#link { color: red; } .primary { font-weight: bold; }";
        let sheet = StyleSheet::parse(css).unwrap();
        let styles = sheet.compute(&doc);
        let a = doc.find_by_tag("a")[0];
        let s = styles.get(&a).unwrap();
        assert_eq!(s.props.get("color"), Some(&"red".to_string()));
        assert_eq!(s.props.get("font-weight"), Some(&"bold".to_string()));
    }

    #[test]
    fn skips_at_rules() {
        let css = r#"
            @media (min-width: 600px) { p { color: red; } }
            @import url(foo.css);
            p { color: green; }
        "#;
        let sheet = StyleSheet::parse(css).unwrap();
        // Только p{color:green} должно быть правилом.
        assert_eq!(sheet.rules.len(), 1);
        assert_eq!(sheet.rules[0].style.props.get("color"), Some(&"green".to_string()));
    }
}
