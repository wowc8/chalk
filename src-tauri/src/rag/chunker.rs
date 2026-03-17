//! Content chunker for splitting lesson plan text into embeddable segments.
//!
//! Uses a simple sentence-boundary chunking strategy with overlap to preserve
//! context across chunk boundaries.

/// A chunk of text extracted from a lesson plan with positional metadata.
#[derive(Debug, Clone)]
pub struct Chunk {
    pub text: String,
    /// Character offset in the original content.
    pub offset: usize,
    /// Index of this chunk within the plan.
    pub index: usize,
}

/// Maximum tokens per chunk (approximate; we use char count as a proxy).
const MAX_CHUNK_CHARS: usize = 1500;
/// Overlap between consecutive chunks to maintain context.
const OVERLAP_CHARS: usize = 200;

/// Split plan content into overlapping chunks suitable for embedding.
///
/// Returns an empty vec if the content is empty or whitespace-only.
pub fn chunk_plan_content(content: &str) -> Vec<Chunk> {
    let content = content.trim();
    if content.is_empty() {
        return Vec::new();
    }

    // Short content: single chunk.
    if content.len() <= MAX_CHUNK_CHARS {
        return vec![Chunk {
            text: content.to_string(),
            offset: 0,
            index: 0,
        }];
    }

    let mut chunks = Vec::new();
    let mut start = 0;
    let mut index = 0;

    while start < content.len() {
        let end = find_chunk_boundary(content, start, MAX_CHUNK_CHARS);
        let text = content[start..end].trim().to_string();

        if !text.is_empty() {
            chunks.push(Chunk {
                text,
                offset: start,
                index,
            });
            index += 1;
        }

        // Move forward, applying overlap.
        let advance = if end - start > OVERLAP_CHARS {
            end - start - OVERLAP_CHARS
        } else {
            end - start
        };
        start += advance;

        // Prevent infinite loops on very small advances.
        if advance == 0 {
            break;
        }
    }

    chunks
}

/// Find the best chunk boundary near `start + max_len`, preferring sentence or
/// paragraph breaks.
fn find_chunk_boundary(content: &str, start: usize, max_len: usize) -> usize {
    let absolute_end = (start + max_len).min(content.len());

    if absolute_end >= content.len() {
        return content.len();
    }

    let window = &content[start..absolute_end];

    // Prefer splitting at paragraph breaks (double newline).
    if let Some(pos) = window.rfind("\n\n") {
        if pos > max_len / 3 {
            return start + pos + 2;
        }
    }

    // Next preference: single newline.
    if let Some(pos) = window.rfind('\n') {
        if pos > max_len / 3 {
            return start + pos + 1;
        }
    }

    // Next: sentence boundary (period + space).
    if let Some(pos) = window.rfind(". ") {
        if pos > max_len / 3 {
            return start + pos + 2;
        }
    }

    // Fallback: hard cut at max_len.
    absolute_end
}

/// Create a composite text for embedding that includes plan metadata.
/// This enriches the embedding with title/objectives context.
pub fn create_embedding_text(title: &str, content: &str, objectives: Option<&str>) -> String {
    let mut parts = Vec::with_capacity(3);
    parts.push(format!("Title: {title}"));
    if let Some(obj) = objectives {
        if !obj.is_empty() {
            parts.push(format!("Objectives: {obj}"));
        }
    }
    parts.push(format!("Content: {content}"));
    parts.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_content_returns_empty() {
        assert!(chunk_plan_content("").is_empty());
        assert!(chunk_plan_content("   ").is_empty());
    }

    #[test]
    fn test_short_content_single_chunk() {
        let chunks = chunk_plan_content("Hello, this is a short plan.");
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].text, "Hello, this is a short plan.");
        assert_eq!(chunks[0].offset, 0);
        assert_eq!(chunks[0].index, 0);
    }

    #[test]
    fn test_long_content_multiple_chunks() {
        // Create content > MAX_CHUNK_CHARS
        let paragraph = "This is a sentence about teaching biology. ";
        let content = paragraph.repeat(100); // ~4400 chars
        let chunks = chunk_plan_content(&content);
        assert!(chunks.len() > 1);

        // Each chunk should be within limit.
        for chunk in &chunks {
            assert!(chunk.text.len() <= MAX_CHUNK_CHARS + 100); // small tolerance
        }

        // Indices should be sequential.
        for (i, chunk) in chunks.iter().enumerate() {
            assert_eq!(chunk.index, i);
        }
    }

    #[test]
    fn test_chunks_have_overlap() {
        let sentences: Vec<String> = (0..60)
            .map(|i| format!("Sentence number {i} about the lesson plan topic. "))
            .collect();
        let content = sentences.join("");
        let chunks = chunk_plan_content(&content);

        if chunks.len() >= 2 {
            // The second chunk should start before the first chunk ends.
            let first_end = chunks[0].offset + chunks[0].text.len();
            assert!(
                chunks[1].offset < first_end,
                "Chunks should overlap: second starts at {} but first ends at {}",
                chunks[1].offset,
                first_end
            );
        }
    }

    #[test]
    fn test_prefers_paragraph_boundary() {
        let part1 = "A".repeat(800);
        let part2 = "B".repeat(800);
        let content = format!("{part1}\n\n{part2}");
        let chunks = chunk_plan_content(&content);
        assert!(chunks.len() >= 2);
        // First chunk should end at paragraph boundary.
        assert!(
            chunks[0].text.ends_with('A'),
            "First chunk should end at paragraph break"
        );
    }

    #[test]
    fn test_create_embedding_text_with_objectives() {
        let text = create_embedding_text("My Plan", "Plan content here", Some("Learn stuff"));
        assert!(text.contains("Title: My Plan"));
        assert!(text.contains("Objectives: Learn stuff"));
        assert!(text.contains("Content: Plan content here"));
    }

    #[test]
    fn test_create_embedding_text_without_objectives() {
        let text = create_embedding_text("My Plan", "Content", None);
        assert!(text.contains("Title: My Plan"));
        assert!(!text.contains("Objectives"));
        assert!(text.contains("Content: Content"));
    }
}
