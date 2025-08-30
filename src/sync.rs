use crate::{diff, patch, Document, EditList, PatchError};
use serde::{Deserialize, Serialize};
use std::fmt;

/// The core synchronization engine implementing differential synchronization
#[derive(Debug, Clone)]
pub struct SyncEngine {
    /// The actual document content that users see and edit
    pub document: Document,
    /// Shadow copy used for diff operations
    shadow: Document,
    /// Backup shadow for guaranteed delivery (server-side)
    backup_shadow: Option<Document>,
    /// Edits waiting to be sent
    pending_edits: Vec<EditList>,
    /// Client/Server identifier
    pub node_id: String,
}

/// Synchronization result
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncResult {
    pub edits: EditList,
    pub shadow_checksum: String,
    pub success: bool,
    pub message: Option<String>,
}

impl SyncEngine {
    /// Create a new synchronization engine
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

    /// Create a new server-side sync engine (with backup shadow)
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

    /// Edit the document content directly (simulates user input)
    pub fn edit(&mut self, new_content: &str) {
        self.document.update(new_content.to_string());
    }

    /// Get the current document text
    pub fn text(&self) -> &str {
        &self.document.content
    }

    /// Get the current document
    pub fn document(&self) -> &Document {
        &self.document
    }

    /// Get the shadow checksum
    pub fn shadow_checksum(&self) -> String {
        crate::diff::checksum(&self.shadow.content)
    }

    /// Generate diff between document and shadow, then update shadow
    /// This is step 1-3 of the sync cycle
    pub fn diff_and_update_shadow(&mut self) -> EditList {
        // Step 1: Diff document against shadow
        let edits = diff(&self.shadow.content, &self.document.content);
        
        // Step 3: Update shadow to match document
        self.shadow = self.document.clone();
        
        edits
    }

    /// Apply incoming edits to the document (step 4-5 of sync cycle)
    pub fn apply_edits(&mut self, edit_list: EditList) -> Result<(), PatchError> {
        if edit_list.is_empty() {
            return Ok(());
        }

        // Apply edits to shadow first (this is the "before" state)
        match patch(&self.shadow.content, &edit_list) {
            Ok(new_shadow_content) => {
                // Update shadow to reflect the synchronized state
                self.shadow.update(new_shadow_content);
                
                // Apply edits to document using fuzzy patching
                match patch(&self.document.content, &edit_list) {
                    Ok(new_content) => {
                        self.document.update(new_content);
                        Ok(())
                    }
                    Err(e) => Err(e),
                }
            }
            Err(e) => Err(e),
        }
    }

    /// Perform a full synchronization cycle with another engine
    pub fn sync_with(&mut self, other: &mut SyncEngine) -> (SyncResult, SyncResult) {
        // Client -> Server sync
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

        // Server -> Client sync
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

    /// Create a backup of the current shadow (for guaranteed delivery)
    pub fn backup_shadow(&mut self) {
        self.backup_shadow = Some(self.shadow.clone());
    }

    /// Restore shadow from backup
    pub fn restore_shadow(&mut self) -> bool {
        if let Some(backup) = &self.backup_shadow {
            self.shadow = backup.clone();
            true
        } else {
            false
        }
    }

    /// Get sync statistics
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
        write!(
            f,
            "SyncEngine[{}]: doc='{}' (v{}), shadow_checksum={}",
            self.node_id,
            if self.document.content.len() > 50 {
                format!("{}...", &self.document.content[..47])
            } else {
                self.document.content.clone()
            },
            self.document.version,
            &self.shadow_checksum()[..8]
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

        // Client edits
        client.edit("Hello beautiful world");

        // Perform sync
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

        // Concurrent edits
        client.edit("The big cat sat on the mat");
        server.edit("The cat sat on the red mat");

        // Perform sync
        let (server_result, client_result) = client.sync_with(&mut server);

        assert!(server_result.success);
        assert!(client_result.success);

        // Both should have both changes
        let final_text = client.text();
        assert!(final_text.contains("big"));
        assert!(final_text.contains("red"));
        assert_eq!(client.text(), server.text());
    }

    #[test]
    fn test_shadow_consistency() {
        let mut engine = SyncEngine::new("Test content".to_string());
        let original_checksum = engine.shadow_checksum();

        // Edit and get diff
        engine.edit("Modified test content");
        let edits = engine.diff_and_update_shadow();

        // Shadow should now match document
        assert_eq!(engine.shadow.content, engine.document.content);
        assert_ne!(engine.shadow_checksum(), original_checksum);
        assert!(!edits.is_empty());
    }
}
