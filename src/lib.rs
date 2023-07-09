// Copyright 2014-2017 The html5ever Project Developers. See the
// COPYRIGHT file at the top-level directory of this distribution.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Based on https://github.com/servo/html5ever/blob/f413b98631f6f2998da48b14ebf34991b45ebcec/rcdom/lib.rs
// and https://github.com/servo/html5ever/blob/f413b98631f6f2998da48b14ebf34991b45ebcec/html5ever/examples/arena.rs
// Modified to use generational_indextree
// The main implementation work here was implementing `TreeSink` for GenerationalArenaDom

use generational_indextree::{Arena as TreeArena, NodeId};

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashSet;
use std::default::Default;

use markup5ever::tendril::StrTendril;

use markup5ever::interface::tree_builder;
use markup5ever::interface::tree_builder::{ElementFlags, NodeOrText, QuirksMode, TreeSink};
use markup5ever::Attribute;
use markup5ever::ExpandedName;
use markup5ever::QualName;

/// The different kinds of nodes in the DOM.
#[derive(Debug)]
pub enum NodeData {
    /// The `Document` itself - the root node of a HTML document.
    Document,

    /// A `DOCTYPE` with name, public id, and system id. See
    /// [document type declaration on wikipedia][dtd wiki].
    ///
    /// [dtd wiki]: https://en.wikipedia.org/wiki/Document_type_declaration
    Doctype {
        name: StrTendril,
        public_id: StrTendril,
        system_id: StrTendril,
    },

    /// A text node.
    Text { contents: RefCell<StrTendril> },

    /// A comment.
    Comment { contents: StrTendril },

    /// An element with attributes.
    Element {
        name: QualName,
        attrs: RefCell<Vec<Attribute>>,

        /// For HTML \<template\> elements, the [template contents].
        ///
        /// [template contents]: https://html.spec.whatwg.org/multipage/#template-contents
        template_contents: RefCell<Option<Handle>>,

        /// Whether the node is a [HTML integration point].
        ///
        /// [HTML integration point]: https://html.spec.whatwg.org/multipage/#html-integration-point
        mathml_annotation_xml_integration_point: bool,
    },

    /// A Processing instruction.
    ProcessingInstruction {
        target: StrTendril,
        contents: StrTendril,
    },
}

/// The Arena holding node data
pub type Arena = TreeArena<NodeData>;

/// Reference to a DOM node.
pub type Handle = NodeId;

fn append_to_existing_text(arena: &Arena, prev: Handle, text: &str) -> bool {
    match arena.get(prev) {
        Some(prev_node) => match prev_node.get() {
            NodeData::Text { ref contents } => {
                contents.borrow_mut().push_slice(text);
                true
            }
            _ => false,
        },
        None => panic!("Node doesn't exist??"),
    }
}

/// The DOM itself; the result of parsing.
pub struct GenerationalArenaDom {
    /// Arena holding the nodes of the Tree
    pub arena: Arena,
    /// The `Document` itself.
    pub document: Handle,

    /// Errors that occurred during parsing.
    pub errors: Vec<Cow<'static, str>>,

    /// The document's quirks mode.
    pub quirks_mode: QuirksMode,
}

impl GenerationalArenaDom {
    fn get_node(&self, target: &Handle) -> &NodeData {
        self.arena.get(*target).expect("Invalid node!").get()
    }

    fn preceding_node(&self, target: &Handle) -> Option<Handle> {
        self.arena
            .get(*target)
            .expect("Invalid node!")
            .previous_sibling()
    }
}

impl TreeSink for GenerationalArenaDom {
    type Output = Self;
    fn finish(self) -> Self {
        self
    }

    type Handle = Handle;

    fn parse_error(&mut self, msg: Cow<'static, str>) {
        self.errors.push(msg);
    }

    fn get_document(&mut self) -> Handle {
        self.document.clone()
    }

    fn elem_name(&self, target: &'_ Handle) -> ExpandedName<'_> {
        return match self.get_node(target) {
            NodeData::Element { ref name, .. } => name.expanded(),
            _ => panic!("not an element!"),
        };
    }

    fn create_element(
        &mut self,
        name: QualName,
        attrs: Vec<Attribute>,
        flags: ElementFlags,
    ) -> Handle {
        let template_inner = if flags.template {
            Some(self.arena.new_node(NodeData::Document))
        } else {
            None
        };
        self.arena.new_node(NodeData::Element {
            name,
            attrs: RefCell::new(attrs),
            template_contents: RefCell::new(template_inner),
            mathml_annotation_xml_integration_point: flags.mathml_annotation_xml_integration_point,
        })
    }

    fn create_comment(&mut self, text: StrTendril) -> Handle {
        self.arena.new_node(NodeData::Comment { contents: text })
    }

    fn create_pi(&mut self, target: StrTendril, data: StrTendril) -> Handle {
        self.arena.new_node(NodeData::ProcessingInstruction {
            target,
            contents: data,
        })
    }

    fn append(&mut self, parent: &Handle, child: NodeOrText<Handle>) {
        let parent_node = self.arena.get(*parent).expect("Invalid node!");
        // Append to an existing Text node if we have one.
        match child {
            NodeOrText::AppendText(ref text) => match parent_node.last_child() {
                Some(h) => {
                    if append_to_existing_text(&self.arena, h, text) {
                        return;
                    }
                }
                _ => (),
            },
            _ => (),
        }

        let new_child = match child {
            NodeOrText::AppendText(text) => self.arena.new_node(NodeData::Text {
                contents: RefCell::new(text),
            }),
            NodeOrText::AppendNode(node) => node,
        };
        parent.append(new_child, &mut self.arena);
    }

    fn append_based_on_parent_node(
        &mut self,
        element: &Self::Handle,
        prev_element: &Self::Handle,
        child: NodeOrText<Self::Handle>,
    ) {
        let element_node = self.arena.get(*element).expect("Invalid handle!");
        let parent = element_node.parent();
        if parent.is_some() {
            self.append_before_sibling(element, child);
        } else {
            self.append(prev_element, child);
        }
    }

    fn append_doctype_to_document(
        &mut self,
        name: StrTendril,
        public_id: StrTendril,
        system_id: StrTendril,
    ) {
        let new_node = self.arena.new_node(NodeData::Doctype {
            name,
            public_id,
            system_id,
        });
        self.document.append(new_node, &mut self.arena)
    }

    fn get_template_contents(&mut self, target: &Handle) -> Handle {
        if let NodeData::Element {
            ref template_contents,
            ..
        } = self.get_node(target)
        {
            template_contents
                .borrow()
                .as_ref()
                .expect("not a template element!")
                .clone()
        } else {
            panic!("not a template element!")
        }
    }

    fn same_node(&self, x: &Handle, y: &Handle) -> bool {
        *x == *y
    }

    fn set_quirks_mode(&mut self, mode: QuirksMode) {
        self.quirks_mode = mode;
    }

    fn append_before_sibling(&mut self, sibling: &Handle, child: NodeOrText<Handle>) {
        let preceding = self.preceding_node(sibling);
        let child = match (child, preceding) {
            // No previous node.
            (NodeOrText::AppendText(text), None) => self.arena.new_node(NodeData::Text {
                contents: RefCell::new(text),
            }),

            // Look for a text node before the insertion point.
            (NodeOrText::AppendText(text), Some(prev)) => {
                if append_to_existing_text(&self.arena, prev, &text) {
                    return;
                }
                self.arena.new_node(NodeData::Text {
                    contents: RefCell::new(text),
                })
            }

            // The tree builder promises we won't have a text node after
            // the insertion point.

            // Any other kind of node.
            (NodeOrText::AppendNode(node), _) => node,
        };
        child.insert_before(*sibling, &mut self.arena);
    }

    fn add_attrs_if_missing(&mut self, target: &Handle, attrs: Vec<Attribute>) {
        let mut existing = if let NodeData::Element { ref attrs, .. } = self.get_node(target) {
            attrs.borrow_mut()
        } else {
            panic!("not an element")
        };

        let existing_names = existing
            .iter()
            .map(|e| e.name.clone())
            .collect::<HashSet<_>>();
        existing.extend(
            attrs
                .into_iter()
                .filter(|attr| !existing_names.contains(&attr.name)),
        );
    }

    fn remove_from_parent(&mut self, target: &Handle) {
        target.detach(&mut self.arena);
    }

    fn reparent_children(&mut self, node: &Handle, new_parent: &Handle) {
        let mut next_child = self
            .arena
            .get_mut(*node)
            .and_then(|node| node.first_child());
        while let Some(child) = next_child {
            child.detach(&mut self.arena);
            new_parent.append(child, &mut self.arena);
            let child_node = self.arena.get_mut(child).unwrap();
            next_child = child_node.next_sibling();
        }
    }

    fn is_mathml_annotation_xml_integration_point(&self, target: &Handle) -> bool {
        if let NodeData::Element {
            mathml_annotation_xml_integration_point,
            ..
        } = self.get_node(target)
        {
            *mathml_annotation_xml_integration_point
        } else {
            panic!("not an element!")
        }
    }
}

impl Default for GenerationalArenaDom {
    fn default() -> GenerationalArenaDom {
        let mut arena = Arena::new();
        let document = arena.new_node(NodeData::Document);
        GenerationalArenaDom {
            arena,
            document,
            errors: vec![],
            quirks_mode: tree_builder::NoQuirks,
        }
    }
}
