/// Render a FlowNode tree as indented text or JSON.
use crate::flow::tree::FlowNode;

pub fn render_text(root: &FlowNode) -> String {
    let mut out = String::new();
    render_node(root, &mut out, "", true, true);
    out
}

fn render_node(node: &FlowNode, out: &mut String, prefix: &str, is_last: bool, is_root: bool) {
    let connector = if is_root {
        ""
    } else if is_last {
        "└── "
    } else {
        "├── "
    };

    let sig_suffix = node
        .signature
        .as_deref()
        .map(|s| {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                String::new()
            } else {
                format!(" {}", trimmed)
            }
        })
        .unwrap_or_default();

    let truncation = node
        .truncated_reason
        .as_deref()
        .map(|r| format!(" [{}]", r.to_uppercase()))
        .unwrap_or_default();

    out.push_str(&format!(
        "{}{}[{}] {}:{}:{} {}{}{}",
        prefix,
        connector,
        node.kind.tag(),
        node.file,
        node.line,
        node.col,
        node.label,
        sig_suffix,
        truncation,
    ));
    out.push('\n');

    let child_prefix = if is_root {
        prefix.to_string()
    } else if is_last {
        format!("{}    ", prefix)
    } else {
        format!("{}│   ", prefix)
    };

    let n = node.children.len();
    for (i, child) in node.children.iter().enumerate() {
        render_node(child, out, &child_prefix, i + 1 == n, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::tree::FlowNodeKind;

    fn leaf(kind: FlowNodeKind, label: &str, line: u32) -> FlowNode {
        FlowNode {
            kind,
            label: label.to_string(),
            file: "main.go".to_string(),
            line,
            col: 1,
            signature: None,
            symbol_id: None,
            children: vec![],
            truncated_reason: None,
        }
    }

    #[test]
    fn test_render_simple() {
        let root = FlowNode {
            kind: FlowNodeKind::Root,
            label: "main".to_string(),
            file: "cmd/main.go".to_string(),
            line: 10,
            col: 1,
            signature: Some("()".to_string()),
            symbol_id: Some(1),
            children: vec![
                leaf(FlowNodeKind::Call, "config.Load", 15),
                leaf(FlowNodeKind::If, "if err != nil", 16),
            ],
            truncated_reason: None,
        };

        let text = render_text(&root);
        assert!(text.contains("[ROOT]"));
        assert!(text.contains("[CALL]"));
        assert!(text.contains("[IF]"));
        assert!(text.contains("config.Load"));
        assert!(text.contains("if err != nil"));
        // Last child uses └──
        assert!(text.contains("└──"));
        // First child uses ├──
        assert!(text.contains("├──"));
    }
}
