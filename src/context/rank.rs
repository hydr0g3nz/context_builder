/// Score and sort expanded nodes.
/// Formula: 0.6 * distance_score + 0.4 * centrality_score
use crate::context::expand::ExpandedNode;

#[derive(Debug, Clone)]
pub struct RankedNode {
    pub node: ExpandedNode,
    pub score: f64,
}

pub fn rank(mut nodes: Vec<ExpandedNode>, limit: usize) -> Vec<RankedNode> {
    if nodes.is_empty() {
        return vec![];
    }

    let max_refs = nodes.iter().map(|n| n.seed_references).max().unwrap_or(1).max(1);

    let mut ranked: Vec<RankedNode> = nodes
        .drain(..)
        .map(|node| {
            let distance_score = 1.0 / (1.0 + node.distance as f64);
            let centrality_score = node.seed_references as f64 / max_refs as f64;
            let score = 0.6 * distance_score + 0.4 * centrality_score;
            RankedNode { node, score }
        })
        .collect();

    ranked.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    ranked.truncate(limit);
    ranked
}
