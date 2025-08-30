pub mod sync;
pub mod diff;
pub mod document;
pub mod persistence;

#[cfg(feature = "network")]
pub mod network;

pub use sync::*;
pub use diff::*;
pub use document::*;
pub use persistence::*;

#[cfg(feature = "network")]
pub use network::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_sync() {
        let mut client = SyncEngine::new("Hello world".to_string());
        let mut server = SyncEngine::new("Hello world".to_string());
        
        // Client makes an edit
        client.edit("Hello beautiful world");
        
        // Sync client -> server
        let edits = client.diff_and_update_shadow();
        server.apply_edits(edits).unwrap();
        
        // Sync server -> client  
        let edits = server.diff_and_update_shadow();
        client.apply_edits(edits).unwrap();
        
        assert_eq!(client.text(), "Hello beautiful world");
        assert_eq!(server.text(), "Hello beautiful world");
    }
}
