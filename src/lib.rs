pub mod diff;
pub mod document;
pub mod persistence;
pub mod sync;

#[cfg(feature = "network")]
pub mod network;
#[cfg(feature = "network")]
pub mod session;

pub use diff::*;
pub use document::*;
pub use persistence::*;
pub use sync::*;

#[cfg(feature = "network")]
pub use network::*;
#[cfg(feature = "network")]
pub use session::*;

/// Truncate text to `max_len` characters, appending "..." if truncated.
/// Operates on char boundaries to avoid splitting multi-byte UTF-8 sequences.
pub fn truncate_text(text: &str, max_len: usize) -> String {
    let char_count = text.chars().count();
    if char_count <= max_len {
        text.to_string()
    } else {
        let truncated: String = text.chars().take(max_len.saturating_sub(3)).collect();
        format!("{truncated}...")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_sync() {
        let mut client = SyncEngine::new("Hello world".to_string());
        let mut server = SyncEngine::new("Hello world".to_string());

        client.edit("Hello beautiful world");

        let edits = client.diff_and_update_shadow();
        server.apply_edits(edits).unwrap();

        let edits = server.diff_and_update_shadow();
        client.apply_edits(edits).unwrap();

        assert_eq!(client.text(), "Hello beautiful world");
        assert_eq!(server.text(), "Hello beautiful world");
    }
}
