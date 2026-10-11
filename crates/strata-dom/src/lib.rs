//! Strata DOM — простое, быстрое DOM-дерево.
//!
//! Не реализует полное соответствие спецификации — только то,
//! что нужно для парсера HTML и применения CSS.

use std::sync::Arc;

use ahash::AHashMap;
use parking_lot::RwLock;
use smol_str::SmolStr;

/// Идентификатор узла в дереве. Используется для эффективного
/// хранения вместо Arc<RwLock<Node>> везде.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodeId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    Document,
    Element,
    Text,
    Comment,
    Doctype,
}

#[derive(Debug, Clone)]
pub struct ElementData {
    pub tag: SmolStr,
    pub attrs: Vec<(SmolStr, SmolStr)>,
    pub id: Option<SmolStr>,
    pub classes: Vec<SmolStr>,
}

impl ElementData {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
}

#[derive(Debug)]
pub struct Node {
    pub kind: NodeKind,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    pub element: Option<ElementData>,
    pub text: Option<SmolStr>,
}

impl Node {
    pub fn empty(kind: NodeKind) -> Self {
        Self {
            kind,
            parent: None,
            children: Vec::new(),
            element: None,
            text: None,
        }
    }
}

#[derive(Debug)]
pub struct Document {
    nodes: Vec<Node>,
    root: NodeId,
    /// Кэш id → NodeId для быстрого поиска по id.
    by_id: AHashMap<SmolStr, NodeId>,
}

impl Document {
    pub fn new() -> Self {
        let mut nodes = Vec::with_capacity(64);
        nodes.push(Node::empty(NodeKind::Document));
        let root = NodeId(0);
        Self {
            nodes,
            root,
            by_id: AHashMap::new(),
        }
    }

    pub fn root(&self) -> NodeId {
        self.root
    }

    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id.0]
    }

    pub fn node_mut(&mut self, id: NodeId) -> &mut Node {
        &mut self.nodes[id.0]
    }

    pub fn append_child(&mut self, parent: NodeId, child: NodeId) {
        self.nodes[child.0].parent = Some(parent);
        self.nodes[parent.0].children.push(child);
    }

    pub fn insert_before(&mut self, parent: NodeId, new_child: NodeId, ref_child: Option<NodeId>) {
        self.nodes[new_child.0].parent = Some(parent);
        match ref_child {
            Some(ref_id) => {
                let idx = self.nodes[parent.0]
                    .children
                    .iter()
                    .position(|c| *c == ref_id)
                    .unwrap_or_else(|| self.nodes[parent.0].children.len());
                self.nodes[parent.0].children.insert(idx, new_child);
            }
            None => self.nodes[parent.0].children.push(new_child),
        }
    }

    /// Создать element узел и вернуть его id.
    pub fn create_element(&mut self, tag: &str, attrs: Vec<(SmolStr, SmolStr)>) -> NodeId {
        let id = self.nodes.len();
        let mut element = ElementData {
            tag: SmolStr::new(tag),
            attrs,
            id: None,
            classes: Vec::new(),
        };

        // Распарсить id / class из attrs для быстрого доступа
        for (k, v) in &element.attrs {
            if k == "id" {
                element.id = Some(v.clone());
            } else if k == "class" {
                element.classes = v
                    .split_whitespace()
                    .map(|c| SmolStr::new(c))
                    .collect();
            }
        }
        let id_clone = element.id.clone();
        let mut node = Node::empty(NodeKind::Element);
        node.element = Some(element);
        self.nodes.push(node);

        if let Some(id_str) = id_clone {
            self.by_id.insert(id_str, NodeId(id));
        }

        NodeId(id)
    }

    pub fn create_text(&mut self, text: &str) -> NodeId {
        let id = self.nodes.len();
        let mut node = Node::empty(NodeKind::Text);
        node.text = Some(SmolStr::new(text));
        self.nodes.push(node);
        NodeId(id)
    }

    pub fn create_comment(&mut self, text: &str) -> NodeId {
        let id = self.nodes.len();
        let mut node = Node::empty(NodeKind::Comment);
        node.text = Some(SmolStr::new(text));
        self.nodes.push(node);
        NodeId(id)
    }

    pub fn create_doctype(&mut self) -> NodeId {
        let id = self.nodes.len();
        let node = Node::empty(NodeKind::Doctype);
        self.nodes.push(node);
        NodeId(id)
    }

    pub fn get_by_id(&self, id: &str) -> Option<NodeId> {
        self.by_id.get(id).copied()
    }

    /// Iterate by tag name (case-insensitive ASCII).
    pub fn find_by_tag(&self, tag: &str) -> Vec<NodeId> {
        let lower = tag.to_ascii_lowercase();
        self.nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| {
                n.kind == NodeKind::Element
                    && n.element
                        .as_ref()
                        .map(|e| e.tag.as_str().eq_ignore_ascii_case(&lower))
                        .unwrap_or(false)
            })
            .map(|(i, _)| NodeId(i))
            .collect()
    }

    pub fn text_content(&self, id: NodeId) -> String {
        let mut out = String::new();
        self.collect_text(id, &mut out);
        out
    }

    fn collect_text(&self, id: NodeId, out: &mut String) {
        let node = self.node(id);
        match node.kind {
            NodeKind::Text => {
                if let Some(t) = &node.text {
                    out.push_str(t);
                }
            }
            NodeKind::Element | NodeKind::Document => {
                for c in node.children.clone() {
                    self.collect_text(c, out);
                }
            }
            _ => {}
        }
    }
}

impl Default for Document {
    fn default() -> Self {
        Self::new()
    }
}

/// Shared-ownership Document. Используется в pipeline.
pub type SharedDocument = Arc<RwLock<Document>>;
