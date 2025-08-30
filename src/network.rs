use crate::{EditList, SyncEngine, Document, DocumentDB};
use colored::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Messages sent between client and server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncMessage {
    /// Client wants to connect and fetch current document
    Connect {
        client_id: String,
    },
    /// Client sends edits to server
    ClientSync {
        client_id: String,
        edits: EditList,
        client_version: u64,
    },
    /// Server sends edits back to client
    ServerSync {
        edits: EditList,
        server_version: u64,
    },
    /// Server confirms connection and provides current document state
    ConnectOk {
        server_version: u64,
        document: Document,
    },
    /// Error occurred
    Error {
        message: String,
    },
    /// Client disconnecting
    Disconnect {
        client_id: String,
    },
    /// Heartbeat to keep connection alive
    Ping,
    Pong,
}

/// Represents a connected client on the server
#[derive(Debug)]
pub struct ClientSession {
    pub client_id: String,
    pub sync_engine: SyncEngine,
    pub last_seen: std::time::Instant,
}

impl ClientSession {
    pub fn new(client_id: String, initial_content: String) -> Self {
        let mut engine = SyncEngine::new(initial_content);
        engine.node_id = client_id.clone();
        
        Self {
            client_id,
            sync_engine: engine,
            last_seen: std::time::Instant::now(),
        }
    }
}

/// Server state managing multiple clients with persistent storage
pub struct SyncServer {
    /// Database for persistent document storage
    pub db: DocumentDB,
    /// Name of the document being edited (support for multiple docs later)
    pub document_name: String,
    /// Connected clients
    pub clients: HashMap<String, ClientSession>,
    /// Server version counter  
    pub version: u64,
}

impl SyncServer {
    /// Create a new server with database persistence
    pub fn new_with_db(db: DocumentDB, document_name: String) -> Result<Self, String> {
        Ok(Self {
            db,
            document_name,
            clients: HashMap::new(),
            version: 0,
        })
    }

    /// Create a new server with in-memory database (for testing)
    pub fn new_in_memory(document_name: String) -> Result<Self, String> {
        let db = DocumentDB::new_in_memory()
            .map_err(|e| format!("Failed to create in-memory database: {}", e))?;
        Self::new_with_db(db, document_name)
    }

    /// Get the current document from database
    pub fn get_current_document(&self) -> Result<Document, String> {
        self.db.load_document(&self.document_name)
            .map_err(|e| format!("Database error: {}", e))?
            .ok_or_else(|| format!("Document '{}' not found", self.document_name))
    }

    /// Handle a client connection
    pub fn connect_client(&mut self, client_id: String) -> Result<Document, String> {
        if self.clients.contains_key(&client_id) {
            return Err(format!("Client {} already connected", client_id));
        }

        // Get current document from database
        let current_doc = self.get_current_document()?;

        // Create new client session with current document content from database
        let session = ClientSession::new(client_id.clone(), current_doc.content.clone());
        
        self.clients.insert(client_id.clone(), session);
        self.version += 1;
        
        println!("✅ Client {} connected (version {})", client_id.green(), self.version);
        Ok(current_doc)
    }

    /// Handle client sync - client sends edits and receives database document updates
    pub fn sync_with_client(&mut self, client_id: &str, client_edits: EditList) -> Result<EditList, String> {
        // First check if client exists
        if !self.clients.contains_key(client_id) {
            return Err(format!("Client {} not found", client_id));
        }

        // Get current document from database before working with client session
        let mut current_doc = self.get_current_document()?;

        // Apply client edits to current document and save to database
        if !client_edits.is_empty() {
            // Apply edits to document content
            let new_content = crate::diff::patch(&current_doc.content, &client_edits)
                .map_err(|e| format!("Failed to apply client edits: {}", e))?;
            
            // Update document in database
            current_doc = self.db.update_document(&self.document_name, new_content)
                .map_err(|e| format!("Failed to save document: {}", e))?;
            
            println!("📝 Client {} updated document (v{})", client_id.green(), current_doc.version);
            self.version += 1;
        }

        // Now get mutable access to client session for shadow operations
        let client_session = self.clients.get_mut(client_id)
            .ok_or_else(|| format!("Client {} not found", client_id))?;

        // Update client's last seen
        client_session.last_seen = std::time::Instant::now();

        // CRITICAL: Apply the SAME edits to client's shadow to keep them in sync
        // This prevents sending the client's own edits back to them
        if !client_edits.is_empty() {
            if let Err(e) = client_session.sync_engine.apply_edits(client_edits.clone()) {
                return Err(format!("Failed to apply client edits to shadow: {}", e));
            }
        }

        // Generate diff from client's current shadow to current database document
        // This will only contain changes from OTHER clients
        let server_edits = crate::diff::diff(
            &client_session.sync_engine.text(),
            &current_doc.content
        );

        // Update client's shadow to match current document
        if !server_edits.is_empty() {
            client_session.sync_engine.edit(&current_doc.content);
            println!("📤 Sending {} edits to client {} (changes from other clients)",
                    server_edits.len().to_string().cyan(), client_id.green());
        }

        Ok(server_edits)
    }

    /// Disconnect a client
    pub fn disconnect_client(&mut self, client_id: &str) {
        if self.clients.remove(client_id).is_some() {
            println!("👋 Client {} disconnected", client_id);
        }
    }

    /// Get current document content from database
    pub fn get_document_content(&self) -> Result<String, String> {
        let doc = self.get_current_document()?;
        Ok(doc.content)
    }

    /// Get list of connected clients
    pub fn get_connected_clients(&self) -> Vec<&str> {
        self.clients.keys().map(|s| s.as_str()).collect()
    }

    /// Remove clients that haven't been seen for a while
    pub fn cleanup_stale_clients(&mut self, timeout_secs: u64) {
        let now = std::time::Instant::now();
        let timeout_duration = std::time::Duration::from_secs(timeout_secs);
        
        let stale_clients: Vec<String> = self.clients
            .iter()
            .filter(|(_, session)| now.duration_since(session.last_seen) > timeout_duration)
            .map(|(id, _)| id.clone())
            .collect();

        for client_id in stale_clients {
            self.disconnect_client(&client_id);
        }
    }
}

/// Shared server state for async handling
pub type SharedSyncServer = Arc<Mutex<SyncServer>>;

/// Serialize a message to JSON bytes
pub fn serialize_message(msg: &SyncMessage) -> Result<Vec<u8>, serde_json::Error> {
    let json = serde_json::to_string(msg)?;
    Ok(format!("{}\n", json).into_bytes())
}

/// Deserialize a message from JSON
pub fn deserialize_message(data: &[u8]) -> Result<SyncMessage, String> {
    let json_str = std::str::from_utf8(data)
        .map_err(|e| e.to_string())?
        .trim();
    let message = serde_json::from_str(json_str)
        .map_err(|e| e.to_string())?;
    Ok(message)
}
