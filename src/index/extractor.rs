use anyhow::Result;
use tree_sitter::{Node, Parser, Query, QueryCursor};

use crate::model::{Symbol, SymbolKind, Visibility};

pub struct GoExtractor {
    parser: Parser,
    query: Query,
}

const GO_QUERY: &str = r#"
(package_clause (package_identifier) @pkg_name)

(function_declaration
  name: (identifier) @func_name
  parameters: (parameter_list) @func_params
  result: (_)? @func_result)

(method_declaration
  receiver: (parameter_list) @recv
  name: (field_identifier) @method_name
  parameters: (parameter_list) @method_params
  result: (_)? @method_result)

(type_declaration
  (type_spec
    name: (type_identifier) @type_name
    type: (struct_type) @struct_body))

(type_declaration
  (type_spec
    name: (type_identifier) @iface_name
    type: (interface_type) @iface_body))

(type_declaration
  (type_spec
    name: (type_identifier) @alias_name
    type: (_) @alias_type))
"#;

impl GoExtractor {
    pub fn new() -> Result<Self> {
        let mut parser = Parser::new();
        let lang = tree_sitter_go::language();
        parser.set_language(&lang)?;
        let lang_ts: tree_sitter::Language = lang.clone();
        let query = Query::new(&lang_ts, GO_QUERY)?;
        Ok(Self { parser, query })
    }

    pub fn extract(&mut self, source: &str, file_path: &str) -> Result<Vec<Symbol>> {
        let tree = self
            .parser
            .parse(source.as_bytes(), None)
            .ok_or_else(|| anyhow::anyhow!("parse failed"))?;

        let root = tree.root_node();
        let bytes = source.as_bytes();
        let mut symbols = vec![];

        let pkg_name = extract_package_name(root, bytes);

        let mut cursor = QueryCursor::new();
        let matches = cursor.matches(&self.query, root, bytes);

        let capture_names = self.query.capture_names();

        for m in matches {
            let caps: Vec<(&str, Node)> = m
                .captures
                .iter()
                .map(|c| (capture_names[c.index as usize], c.node))
                .collect();

            if let Some(sym) = build_symbol(&caps, bytes, file_path, &pkg_name) {
                symbols.push(sym);
            }
        }

        Ok(symbols)
    }
}

fn extract_package_name(root: Node, bytes: &[u8]) -> String {
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() == "package_clause" {
            let mut cc = child.walk();
            for grandchild in child.children(&mut cc) {
                if grandchild.kind() == "package_identifier" {
                    if let Ok(name) = grandchild.utf8_text(bytes) {
                        return name.to_string();
                    }
                }
            }
        }
    }
    "main".to_string()
}

fn build_symbol(
    caps: &[(&str, Node)],
    bytes: &[u8],
    file: &str,
    pkg: &str,
) -> Option<Symbol> {
    fn text(node: Node, bytes: &[u8]) -> String {
        node.utf8_text(bytes).unwrap_or("").to_string()
    }

    fn trunc(s: String, max: usize) -> String {
        if s.len() > max {
            format!("{}…", &s[..max])
        } else {
            s
        }
    }

    fn extract_doc(node: Node, bytes: &[u8]) -> Option<String> {
        let parent = node.parent()?;
        let mut prev = parent.prev_sibling();
        let mut comments = vec![];
        while let Some(sib) = prev {
            if sib.kind() == "comment" {
                let text = sib.utf8_text(bytes).unwrap_or("").to_string();
                comments.push(text.trim_start_matches("//").trim().to_string());
                prev = sib.prev_sibling();
            } else {
                break;
            }
        }
        if comments.is_empty() {
            None
        } else {
            comments.reverse();
            Some(trunc(comments.join(" "), 300))
        }
    }

    if let Some((_, name_node)) = caps.iter().find(|(n, _)| *n == "func_name") {
        let name = text(*name_node, bytes);
        let params = caps
            .iter()
            .find(|(n, _)| *n == "func_params")
            .map(|(_, nd)| text(*nd, bytes))
            .unwrap_or_default();
        let result = caps
            .iter()
            .find(|(n, _)| *n == "func_result")
            .map(|(_, nd)| text(*nd, bytes))
            .unwrap_or_default();
        let sig = if result.is_empty() {
            trunc(params, 200)
        } else {
            trunc(format!("{} {}", params, result), 200)
        };
        let start = name_node.start_position();
        let line_end = name_node.parent().map(|p| p.end_position().row as u32 + 1);
        return Some(Symbol {
            id: None,
            kind: SymbolKind::Func,
            visibility: Visibility::from_name(&name),
            doc: extract_doc(*name_node, bytes),
            name,
            package: pkg.to_string(),
            file: file.to_string(),
            line: start.row as u32 + 1,
            col: start.column as u32 + 1,
            line_end,
            signature: Some(sig),
            hash: None,
        });
    }

    if let Some((_, name_node)) = caps.iter().find(|(n, _)| *n == "method_name") {
        let method_name = text(*name_node, bytes);
        let recv_text = caps
            .iter()
            .find(|(n, _)| *n == "recv")
            .map(|(_, nd)| text(*nd, bytes))
            .unwrap_or_default();
        let receiver_type = parse_receiver_type(&recv_text);
        let full_name = format!("{}.{}", receiver_type, method_name);
        let params = caps
            .iter()
            .find(|(n, _)| *n == "method_params")
            .map(|(_, nd)| text(*nd, bytes))
            .unwrap_or_default();
        let result = caps
            .iter()
            .find(|(n, _)| *n == "method_result")
            .map(|(_, nd)| text(*nd, bytes))
            .unwrap_or_default();
        let sig = if result.is_empty() {
            trunc(params, 200)
        } else {
            trunc(format!("{} {}", params, result), 200)
        };
        let start = name_node.start_position();
        // parent of field_identifier is method_declaration
        let line_end = name_node
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.end_position().row as u32 + 1);
        return Some(Symbol {
            id: None,
            kind: SymbolKind::Method,
            visibility: Visibility::from_name(&method_name),
            doc: extract_doc(*name_node, bytes),
            name: full_name,
            package: pkg.to_string(),
            file: file.to_string(),
            line: start.row as u32 + 1,
            col: start.column as u32 + 1,
            line_end,
            signature: Some(sig),
            hash: None,
        });
    }

    if let Some((_, name_node)) = caps.iter().find(|(n, _)| *n == "type_name") {
        let name = text(*name_node, bytes);
        let start = name_node.start_position();
        return Some(Symbol {
            id: None,
            kind: SymbolKind::Struct,
            visibility: Visibility::from_name(&name),
            doc: extract_doc(*name_node, bytes),
            name,
            package: pkg.to_string(),
            file: file.to_string(),
            line: start.row as u32 + 1,
            col: start.column as u32 + 1,
            line_end: None,
            signature: None,
            hash: None,
        });
    }

    if let Some((_, name_node)) = caps.iter().find(|(n, _)| *n == "iface_name") {
        let name = text(*name_node, bytes);
        let start = name_node.start_position();
        return Some(Symbol {
            id: None,
            kind: SymbolKind::Interface,
            visibility: Visibility::from_name(&name),
            doc: extract_doc(*name_node, bytes),
            name,
            package: pkg.to_string(),
            file: file.to_string(),
            line: start.row as u32 + 1,
            col: start.column as u32 + 1,
            line_end: None,
            signature: None,
            hash: None,
        });
    }

    if let Some((_, name_node)) = caps.iter().find(|(n, _)| *n == "alias_name") {
        let name = text(*name_node, bytes);
        let alias_type_node = caps.iter().find(|(n, _)| *n == "alias_type").map(|(_, nd)| *nd);
        // skip if the type is struct or interface — those are handled by their own patterns
        if let Some(atn) = alias_type_node {
            let kind = atn.kind();
            if kind == "struct_type" || kind == "interface_type" {
                return None;
            }
        }
        let alias_type = alias_type_node.map(|nd| text(nd, bytes));
        let start = name_node.start_position();
        return Some(Symbol {
            id: None,
            kind: SymbolKind::TypeAlias,
            visibility: Visibility::from_name(&name),
            doc: extract_doc(*name_node, bytes),
            name,
            package: pkg.to_string(),
            file: file.to_string(),
            line: start.row as u32 + 1,
            col: start.column as u32 + 1,
            line_end: None,
            signature: alias_type,
            hash: None,
        });
    }

    None
}

fn parse_receiver_type(recv: &str) -> String {
    // recv looks like "(s *UserService)" or "(b *bytes.Buffer)"
    let inner = recv.trim_matches(|c| c == '(' || c == ')').trim();
    // take the last token (the type)
    let type_part = inner.split_whitespace().last().unwrap_or(inner);
    // strip leading * (pointer receiver)
    type_part.trim_start_matches('*').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extract(src: &str) -> Vec<Symbol> {
        let mut ex = GoExtractor::new().unwrap();
        ex.extract(src, "test.go").unwrap()
    }

    #[test]
    fn test_func() {
        let syms = extract(
            r#"package main
func Hello(name string) string { return name }
"#,
        );
        let f = syms.iter().find(|s| s.name == "Hello").unwrap();
        assert_eq!(f.kind, SymbolKind::Func);
        assert_eq!(f.visibility, Visibility::Exported);
        assert_eq!(f.line, 2);
    }

    #[test]
    fn test_private_func() {
        let syms = extract(
            r#"package main
func hello() {}
"#,
        );
        let f = syms.iter().find(|s| s.name == "hello").unwrap();
        assert_eq!(f.visibility, Visibility::Private);
    }

    #[test]
    fn test_struct() {
        let syms = extract(
            r#"package user
type UserService struct { db *DB }
"#,
        );
        let s = syms.iter().find(|s| s.name == "UserService").unwrap();
        assert_eq!(s.kind, SymbolKind::Struct);
        assert_eq!(s.package, "user");
    }

    #[test]
    fn test_interface() {
        let syms = extract(
            r#"package io
type Reader interface { Read(p []byte) (n int, err error) }
"#,
        );
        let i = syms.iter().find(|s| s.name == "Reader").unwrap();
        assert_eq!(i.kind, SymbolKind::Interface);
    }

    #[test]
    fn test_method_pointer_receiver() {
        let syms = extract(
            r#"package svc
type UserService struct{}
func (s *UserService) Save(ctx context.Context, u *User) error { return nil }
"#,
        );
        let m = syms.iter().find(|s| s.name == "UserService.Save").unwrap();
        assert_eq!(m.kind, SymbolKind::Method);
        assert_eq!(m.visibility, Visibility::Exported);
    }

    #[test]
    fn test_method_value_receiver() {
        let syms = extract(
            r#"package svc
type Foo struct{}
func (f Foo) bar() {}
"#,
        );
        let m = syms.iter().find(|s| s.name == "Foo.bar").unwrap();
        assert_eq!(m.visibility, Visibility::Private);
    }

    #[test]
    fn test_type_alias() {
        let syms = extract(
            r#"package http
type Handler func(ResponseWriter, *Request)
"#,
        );
        let a = syms.iter().find(|s| s.name == "Handler").unwrap();
        assert_eq!(a.kind, SymbolKind::TypeAlias);
    }

    #[test]
    fn test_doc_comment() {
        let syms = extract(
            r#"package main
// Hello greets a person by name.
func Hello(name string) string { return name }
"#,
        );
        let f = syms.iter().find(|s| s.name == "Hello").unwrap();
        assert!(f.doc.as_deref().unwrap_or("").contains("Hello greets"));
    }
}
