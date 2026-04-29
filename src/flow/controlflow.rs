/// Extract control-flow nodes from a Go function body using tree-sitter.
use anyhow::Result;
use tree_sitter::{Node, Parser, Query, QueryCursor};

#[derive(Debug, Clone, serde::Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CfKind {
    If,
    Else,
    Switch,
    TypeSwitch,
    Select,
    CommCase,
    Go,
    Defer,
}

impl CfKind {
    pub fn tag(&self) -> &'static str {
        match self {
            CfKind::If => "IF",
            CfKind::Else => "ELSE",
            CfKind::Switch => "SWITCH",
            CfKind::TypeSwitch => "TYPESWITCH",
            CfKind::Select => "SELECT",
            CfKind::CommCase => "CASE",
            CfKind::Go => "GO",
            CfKind::Defer => "DEFER",
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ControlFlowNode {
    pub kind: CfKind,
    pub line: u32,
    pub col: u32,
    pub end_line: u32,
    /// First line of the construct, trimmed to 80 chars
    pub label: String,
}

const CF_QUERY: &str = r#"
(if_statement) @if
(expression_switch_statement) @switch
(type_switch_statement) @typeswitch
(select_statement) @select
(communication_case) @comm_case
(go_statement) @go
(defer_statement) @defer
"#;

pub struct ControlFlowExtractor {
    parser: Parser,
    query: Query,
}

impl ControlFlowExtractor {
    pub fn new() -> Result<Self> {
        let mut parser = Parser::new();
        let lang = tree_sitter_go::language();
        parser.set_language(&lang)?;
        let lang_ts: tree_sitter::Language = lang.clone();
        let query = Query::new(&lang_ts, CF_QUERY)?;
        Ok(Self { parser, query })
    }

    /// Extract all control-flow nodes whose start line is within [start_line, end_line] (1-indexed).
    pub fn extract_in_range(
        &mut self,
        source: &str,
        start_line: u32,
        end_line: u32,
    ) -> Result<Vec<ControlFlowNode>> {
        let tree = self
            .parser
            .parse(source.as_bytes(), None)
            .ok_or_else(|| anyhow::anyhow!("tree-sitter parse failed"))?;

        let bytes = source.as_bytes();
        let root = tree.root_node();

        let capture_names = self.query.capture_names();
        let mut cursor = QueryCursor::new();
        let matches = cursor.matches(&self.query, root, bytes);

        let lines: Vec<&str> = source.lines().collect();

        let mut nodes: Vec<ControlFlowNode> = Vec::new();
        for m in matches {
            for cap in m.captures {
                let node: Node = cap.node;
                let node_line = node.start_position().row as u32 + 1;
                // Only include nodes whose start line falls within the function body
                if node_line < start_line || node_line > end_line {
                    continue;
                }

                let kind_name = capture_names[cap.index as usize];
                let kind = match kind_name {
                    "if" => CfKind::If,
                    "switch" => CfKind::Switch,
                    "typeswitch" => CfKind::TypeSwitch,
                    "select" => CfKind::Select,
                    "comm_case" => CfKind::CommCase,
                    "go" => CfKind::Go,
                    "defer" => CfKind::Defer,
                    _ => continue,
                };

                let col = node.start_position().column as u32 + 1;
                let end_line_node = node.end_position().row as u32 + 1;

                // First source line of the construct, trimmed
                let label = lines
                    .get((node_line - 1) as usize)
                    .map(|l| {
                        let trimmed = l.trim();
                        if trimmed.len() > 80 {
                            format!("{}…", &trimmed[..80])
                        } else {
                            trimmed.to_string()
                        }
                    })
                    .unwrap_or_default();

                nodes.push(ControlFlowNode {
                    kind,
                    line: node_line,
                    col,
                    end_line: end_line_node,
                    label,
                });
            }
        }

        // Sort by line, then col for deterministic output
        nodes.sort_by_key(|n| (n.line, n.col));
        // Deduplicate — same line/col/kind can appear from overlapping captures
        nodes.dedup_by_key(|n| (n.line, n.col, n.kind.clone()));

        Ok(nodes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extract(src: &str, start: u32, end: u32) -> Vec<ControlFlowNode> {
        let mut ex = ControlFlowExtractor::new().unwrap();
        ex.extract_in_range(src, start, end).unwrap()
    }

    #[test]
    fn test_if_detected() {
        let src = r#"package main
func foo() {
    if err != nil {
        return
    }
}
"#;
        let nodes = extract(src, 2, 6);
        assert!(nodes.iter().any(|n| n.kind == CfKind::If));
    }

    #[test]
    fn test_go_defer() {
        let src = r#"package main
func bar() {
    defer cleanup()
    go process()
}
"#;
        let nodes = extract(src, 2, 5);
        assert!(nodes.iter().any(|n| n.kind == CfKind::Defer));
        assert!(nodes.iter().any(|n| n.kind == CfKind::Go));
    }

    #[test]
    fn test_switch() {
        let src = r#"package main
func baz(x int) {
    switch x {
    case 1:
    case 2:
    }
}
"#;
        let nodes = extract(src, 2, 7);
        assert!(nodes.iter().any(|n| n.kind == CfKind::Switch));
    }

    #[test]
    fn test_out_of_range_excluded() {
        let src = r#"package main
func a() {
    if true {}
}
func b() {
    if false {}
}
"#;
        // Only extract inside func a (lines 2-4)
        let nodes = extract(src, 2, 4);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].line, 3);
    }
}
