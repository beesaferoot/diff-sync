use crate::{Document, DocumentDB, EditList, SyncEngine};
use colored::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// Cursor position and display color for a connected client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorInfo {
    pub client_id: String,
    pub position: usize,
    pub color: String,
}

/// Wire protocol between client and server, serialized as externally-tagged JSON
/// (serde default).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncMessage {
    Connect {
        client_id: String,
    },

    ClientSync {
        client_id: String,
        edits: EditList,
        client_version: u64,
        #[serde(default)]
        cursor_position: Option<usize>,
    },

    ServerSync {
        edits: EditList,
        server_version: u64,
        #[serde(default)]
        cursors: Vec<CursorInfo>,
    },

    ConnectOk {
        server_version: u64,
        document: Document,
    },

    Error {
        message: String,
    },
    Disconnect {
        client_id: String,
    },
    /// Sent to every connected client when the session is closed by its creator.
    SessionClosed,
    Ping,
    Pong,
}

/// Server-side state for a single connected client.
#[derive(Debug)]
pub struct ClientSession {
    pub client_id: String,
    pub sync_engine: SyncEngine,
    pub last_seen: Instant,
    pub cursor_position: Option<usize>,
    pub color: String,
}

impl ClientSession {
    pub fn new(client_id: String, initial_content: String, color: String) -> Self {
        let mut engine = SyncEngine::new(initial_content);
        engine.node_id = client_id.clone();
        Self {
            client_id,
            sync_engine: engine,
            last_seen: Instant::now(),
            cursor_position: None,
            color,
        }
    }
}

/// Authoritative server managing multiple clients against a persistent document.
///
/// Each client gets its own `SyncEngine` shadow so the server can compute
/// per-client diffs containing only edits from *other* clients.
pub struct SyncServer {
    pub db: DocumentDB,
    pub document_name: String,
    pub clients: HashMap<String, ClientSession>,
    pub version: u64,
}

impl SyncServer {
    pub fn new_with_db(db: DocumentDB, document_name: String) -> Result<Self, String> {
        Ok(Self {
            db,
            document_name,
            clients: HashMap::new(),
            version: 0,
        })
    }

    pub fn new_in_memory(document_name: String) -> Result<Self, String> {
        let db = DocumentDB::new_in_memory()
            .map_err(|e| format!("Failed to create in-memory database: {e}"))?;
        Self::new_with_db(db, document_name)
    }

    pub fn get_current_document(&self) -> Result<Document, String> {
        self.db
            .load_document(&self.document_name)
            .map_err(|e| format!("Database error: {e}"))?
            .ok_or_else(|| format!("Document '{}' not found", self.document_name))
    }

    pub fn connect_client(&mut self, client_id: String) -> Result<Document, String> {
        if self.clients.contains_key(&client_id) {
            return Err(format!("Client {client_id} already connected"));
        }

        let current_doc = self.get_current_document()?;
        let color = random_cursor_color();
        let session = ClientSession::new(client_id.clone(), current_doc.content.clone(), color);

        self.clients.insert(client_id.clone(), session);
        self.version += 1;

        println!("Client {} connected (v{})", client_id.green(), self.version);
        Ok(current_doc)
    }

    /// Process a client sync: apply the client's edits to the DB, then diff
    /// the client's shadow against the (possibly updated) DB document to
    /// produce edits containing only changes from *other* clients.
    pub fn sync_with_client(
        &mut self,
        client_id: &str,
        client_edits: EditList,
    ) -> Result<EditList, String> {
        if !self.clients.contains_key(client_id) {
            return Err(format!("Client {client_id} not found"));
        }

        let mut current_doc = self.get_current_document()?;

        if !client_edits.is_empty() {
            let new_content = crate::diff::patch(&current_doc.content, &client_edits)
                .map_err(|e| format!("Failed to apply client edits: {e}"))?;

            current_doc = self
                .db
                .update_document(&self.document_name, new_content)
                .map_err(|e| format!("Failed to save document: {e}"))?;

            println!(
                "Client {} updated document (v{})",
                client_id.green(),
                current_doc.version
            );
            self.version += 1;
        }

        let session = self
            .clients
            .get_mut(client_id)
            .ok_or_else(|| format!("Client {client_id} not found"))?;

        session.last_seen = Instant::now();

        // Keep the client's shadow in sync by applying the same edits
        if !client_edits.is_empty() {
            session
                .sync_engine
                .apply_edits(client_edits)
                .map_err(|e| format!("Failed to apply client edits to shadow: {e}"))?;
        }

        // Diff shadow against DB document — only other clients' changes remain
        let server_edits = crate::diff::diff(session.sync_engine.text(), &current_doc.content);

        if !server_edits.is_empty() {
            session.sync_engine.edit(&current_doc.content);
            println!(
                "Sending {} edits to client {}",
                server_edits.len().to_string().cyan(),
                client_id.green()
            );
        }

        Ok(server_edits)
    }

    pub fn update_cursor(&mut self, client_id: &str, position: usize) {
        if let Some(session) = self.clients.get_mut(client_id) {
            session.cursor_position = Some(position);
        }
    }

    /// Return cursor info for every client except `exclude_client`.
    pub fn get_cursors_for(&self, exclude_client: &str) -> Vec<CursorInfo> {
        self.clients
            .values()
            .filter_map(|s| {
                if s.client_id == exclude_client {
                    return None;
                }
                Some(CursorInfo {
                    client_id: s.client_id.clone(),
                    position: s.cursor_position?,
                    color: s.color.clone(),
                })
            })
            .collect()
    }

    pub fn disconnect_client(&mut self, client_id: &str) {
        if self.clients.remove(client_id).is_some() {
            println!("Client {} disconnected", client_id);
        }
    }

    pub fn get_document_content(&self) -> Result<String, String> {
        Ok(self.get_current_document()?.content)
    }

    pub fn get_connected_clients(&self) -> Vec<&str> {
        self.clients.keys().map(|s| s.as_str()).collect()
    }

    pub fn cleanup_stale_clients(&mut self, timeout_secs: u64) {
        let timeout = Duration::from_secs(timeout_secs);
        let now = Instant::now();

        let stale: Vec<String> = self
            .clients
            .iter()
            .filter(|(_, s)| now.duration_since(s.last_seen) > timeout)
            .map(|(id, _)| id.clone())
            .collect();

        for id in stale {
            self.disconnect_client(&id);
        }
    }
}

/// Thread-safe handle to the server, shared across async tasks and connections.
pub type SharedSyncServer = Arc<Mutex<SyncServer>>;

/// Serialize a `SyncMessage` to newline-delimited JSON bytes (for TCP framing).
pub fn serialize_message(msg: &SyncMessage) -> Result<Vec<u8>, serde_json::Error> {
    let mut json = serde_json::to_string(msg)?;
    json.push('\n');
    Ok(json.into_bytes())
}

/// Deserialize a `SyncMessage` from JSON bytes.
pub fn deserialize_message(data: &[u8]) -> Result<SyncMessage, String> {
    let json_str = std::str::from_utf8(data).map_err(|e| e.to_string())?.trim();
    serde_json::from_str(json_str).map_err(|e| e.to_string())
}

/// Route an incoming message to the appropriate server handler and return the
/// response (if any). Shared by both TCP and WebSocket transports.
pub async fn handle_sync_message(
    message: SyncMessage,
    server: &SharedSyncServer,
    client_id: &mut Option<String>,
) -> Option<SyncMessage> {
    match message {
        SyncMessage::Connect { client_id: id } => {
            println!("Client {} requesting connection", id.green());
            let mut server_lock = server.lock().await;
            match server_lock.connect_client(id.clone()) {
                Ok(document) => {
                    *client_id = Some(id);
                    Some(SyncMessage::ConnectOk {
                        server_version: server_lock.version,
                        document,
                    })
                }
                Err(e) => Some(SyncMessage::Error { message: e }),
            }
        }

        SyncMessage::ClientSync {
            client_id: id,
            edits,
            cursor_position,
            ..
        } => {
            let mut server_lock = server.lock().await;

            if let Some(pos) = cursor_position {
                server_lock.update_cursor(&id, pos);
            }

            if !edits.is_empty() {
                println!(
                    "Client {} syncing {} edits",
                    id.cyan(),
                    edits.len().to_string().yellow()
                );
            }

            match server_lock.sync_with_client(&id, edits) {
                Ok(server_edits) => {
                    let cursors = server_lock.get_cursors_for(&id);
                    Some(SyncMessage::ServerSync {
                        edits: server_edits,
                        server_version: server_lock.version,
                        cursors,
                    })
                }
                Err(e) => Some(SyncMessage::Error { message: e }),
            }
        }

        SyncMessage::Disconnect { client_id: id } => {
            server.lock().await.disconnect_client(&id);
            None
        }

        SyncMessage::Ping => Some(SyncMessage::Pong),

        _ => Some(SyncMessage::Error {
            message: "Unexpected message type".to_string(),
        }),
    }
}

/// Generate a random saturated HSL color for cursor display.
fn random_cursor_color() -> String {
    let hue = rand::random::<u16>() % 360;
    let sat = 60 + (rand::random::<u8>() % 30);
    let lit = 45 + (rand::random::<u8>() % 15);
    format!("hsl({hue}, {sat}%, {lit}%)")
}
