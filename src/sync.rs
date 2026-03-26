use crate::{diff, patch, Document, EditList, PatchError};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Core differential synchronization engine.
///
/// Maintains three copies of the document per Neil Fraser's algorithm:
/// - **document**: the live working copy that users edit
/// - **shadow**: last state agreed upon with the remote peer
/// - **backup_shadow** (optional): safety net for guaranteed delivery on the server side
///
/// A sync cycle diffs document against shadow to produce outgoing edits, then
/// applies incoming edits to both shadow and document.
#[derive(Debug, Clone)]
pub struct SyncEngine {
    pub document: Document,
    shadow: Document,
    backup_shadow: Option<Document>,
    pending_edits: Vec<EditList>,
    pub node_id: String,
}

/// Result of one direction of a sync cycle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncResult {
    pub edits: EditList,
    pub shadow_checksum: String,
    pub success: bool,
    pub message: Option<String>,
}

impl SyncEngine {
    pub fn new(content: String) -> Self {
        let document = Document::new(content.clone());
        let shadow = Document::new(content);
        Self {
            document,
            shadow,
            backup_shadow: None,
            pending_edits: Vec::new(),
            node_id: format!("node_{}", rand::random::<u32>()),
        }
    }

    /// Create a server-side engine with backup shadow for guaranteed delivery.
    pub fn new_server(content: String, node_id: String) -> Self {
        let document = Document::new(content.clone());
        let shadow = Document::new(content.clone());
        let backup_shadow = Some(Document::new(content));
        Self {
            document,
            shadow,
            backup_shadow,
            pending_edits: Vec::new(),
            node_id,
        }
    }

    pub fn edit(&mut self, new_content: &str) {
        self.document.update(new_content.to_string());
    }

    pub fn text(&self) -> &str {
        &self.document.content
    }

    pub fn document(&self) -> &Document {
        &self.document
    }

    pub fn shadow_checksum(&self) -> String {
        crate::diff::checksum(&self.shadow.content)
    }

    /// Diff document against shadow and advance the shadow to match.
    /// Returns the edits representing local changes since the last sync.
    pub fn diff_and_update_shadow(&mut self) -> EditList {
        let edits = diff(&self.shadow.content, &self.document.content);
        self.shadow = self.document.clone();
        edits
    }

    /// Apply incoming edits from a remote peer to both shadow and document.
    pub fn apply_edits(&mut self, edit_list: EditList) -> Result<(), PatchError> {
        if edit_list.is_empty() {
            return Ok(());
        }

        let new_shadow = patch(&self.shadow.content, &edit_list)?;
        self.shadow.update(new_shadow);

        let new_doc = patch(&self.document.content, &edit_list)?;
        self.document.update(new_doc);

        Ok(())
    }

    /// Run a full bidirectional sync cycle between `self` and `other`.
    pub fn sync_with(&mut self, other: &mut SyncEngine) -> (SyncResult, SyncResult) {
        let client_edits = self.diff_and_update_shadow();
        let server_result = match other.apply_edits(client_edits.clone()) {
            Ok(()) => SyncResult {
                edits: client_edits,
                shadow_checksum: other.shadow_checksum(),
                success: true,
                message: None,
            },
            Err(e) => SyncResult {
                edits: client_edits,
                shadow_checksum: other.shadow_checksum(),
                success: false,
                message: Some(e.to_string()),
            },
        };

        let server_edits = other.diff_and_update_shadow();
        let client_result = match self.apply_edits(server_edits.clone()) {
            Ok(()) => SyncResult {
                edits: server_edits,
                shadow_checksum: self.shadow_checksum(),
                success: true,
                message: None,
            },
            Err(e) => SyncResult {
                edits: server_edits,
                shadow_checksum: self.shadow_checksum(),
                success: false,
                message: Some(e.to_string()),
            },
        };

        (server_result, client_result)
    }

    pub fn backup_shadow(&mut self) {
        self.backup_shadow = Some(self.shadow.clone());
    }

    pub fn restore_shadow(&mut self) -> bool {
        if let Some(backup) = self.backup_shadow.take() {
            self.shadow = backup;
            true
        } else {
            false
        }
    }

    pub fn stats(&self) -> SyncStats {
        SyncStats {
            document_version: self.document.version,
            document_length: self.document.len(),
            shadow_checksum: self.shadow_checksum(),
            has_backup: self.backup_shadow.is_some(),
            pending_edits: self.pending_edits.len(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncStats {
    pub document_version: u64,
    pub document_length: usize,
    pub shadow_checksum: String,
    pub has_backup: bool,
    pub pending_edits: usize,
}

impl fmt::Display for SyncEngine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let content = crate::truncate_text(&self.document.content, 50);
        let checksum = self.shadow_checksum();
        let short_checksum = &checksum[..checksum.len().min(8)];
        write!(
            f,
            "SyncEngine[{}]: doc='{content}' (v{}), shadow_checksum={short_checksum}",
            self.node_id, self.document.version,
        )
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
        let (server_result, client_result) = client.sync_with(&mut server);

        assert!(server_result.success);
        assert!(client_result.success);
        assert_eq!(client.text(), "Hello beautiful world");
        assert_eq!(server.text(), "Hello beautiful world");
    }

    #[test]
    fn test_concurrent_edits() {
        let mut client = SyncEngine::new("The cat sat on the mat".to_string());
        let mut server = SyncEngine::new("The cat sat on the mat".to_string());

        client.edit("The big cat sat on the mat");
        server.edit("The cat sat on the red mat");

        let (server_result, client_result) = client.sync_with(&mut server);

        assert!(server_result.success);
        assert!(client_result.success);

        let final_text = client.text();
        assert!(final_text.contains("big"));
        assert!(final_text.contains("red"));
        assert_eq!(client.text(), server.text());
    }

    #[test]
    fn test_shadow_consistency() {
        let mut engine = SyncEngine::new("Test content".to_string());
        let original_checksum = engine.shadow_checksum();

        engine.edit("Modified test content");
        let edits = engine.diff_and_update_shadow();

        assert_eq!(engine.shadow.content, engine.document.content);
        assert_ne!(engine.shadow_checksum(), original_checksum);
        assert!(!edits.is_empty());
    }
}
