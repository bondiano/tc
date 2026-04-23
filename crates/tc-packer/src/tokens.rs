/// Estimate token count using chars/4 heuristic (~15% accuracy).
pub fn estimate_tokens(content: &str) -> usize {
    content.len() / 4
}
