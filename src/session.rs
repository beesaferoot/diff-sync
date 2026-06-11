use crate::{DocumentDB, SharedSyncServer, SyncServer};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, Mutex};

pub struct SessionManager {
    db_path: String,
    sessions: HashMap<String, SessionEntry>,
    default_server: Option<SharedSyncServer>,
}

struct SessionEntry {
    server: SharedSyncServer,
    last_active: Instant,
    shutdown: broadcast::Sender<()>,
}

pub type SharedSessionManager = Arc<Mutex<SessionManager>>;

impl SessionManager {
    pub fn new(db_path: String) -> Self {
        Self {
            db_path,
            sessions: HashMap::new(),
            default_server: None,
        }
    }

    pub fn default_server(&mut self) -> Result<SharedSyncServer, String> {
        if let Some(ref server) = self.default_server {
            return Ok(Arc::clone(server));
        }
        let db =
            DocumentDB::new(&self.db_path).map_err(|e| format!("Failed to open database: {e}"))?;
        let server: SharedSyncServer =
            Arc::new(Mutex::new(SyncServer::new_with_db(db, "main".to_string())?));
        self.default_server = Some(Arc::clone(&server));
        Ok(server)
    }

    pub fn create_session(&self, initial_content: &str) -> Result<(String, String), String> {
        let token = generate_token();
        let creator_secret = generate_token();

        let db =
            DocumentDB::new(&self.db_path).map_err(|e| format!("Failed to open database: {e}"))?;
        db.create_session(&token, &creator_secret, initial_content)
            .map_err(|e| format!("Failed to create session: {e}"))?;

        Ok((token, creator_secret))
    }

    /// Returns the session's server and a receiver that fires when it's closed.
    pub fn get_or_start_session(
        &mut self,
        token: &str,
    ) -> Result<(SharedSyncServer, broadcast::Receiver<()>), SessionError> {
        if let Some(entry) = self.sessions.get_mut(token) {
            entry.last_active = Instant::now();
            return Ok((Arc::clone(&entry.server), entry.shutdown.subscribe()));
        }

        let db = DocumentDB::new(&self.db_path)
            .map_err(|e| SessionError::Internal(format!("Failed to open database: {e}")))?;

        let session = db
            .get_session(token)
            .map_err(|e| SessionError::Internal(format!("Database error: {e}")))?
            .ok_or(SessionError::NotFound)?;

        if session.status != "active" {
            return Err(SessionError::Closed);
        }

        let server: SharedSyncServer = Arc::new(Mutex::new(
            SyncServer::new_with_db(db, session.document_name)
                .map_err(|e| SessionError::Internal(e))?,
        ));

        let (shutdown, rx) = broadcast::channel(1);

        self.sessions.insert(
            token.to_string(),
            SessionEntry {
                server: Arc::clone(&server),
                last_active: Instant::now(),
                shutdown,
            },
        );

        Ok((server, rx))
    }

    pub async fn close_session(
        &mut self,
        token: &str,
        creator_secret: &str,
    ) -> Result<(), SessionError> {
        let db = DocumentDB::new(&self.db_path)
            .map_err(|e| SessionError::Internal(format!("Failed to open database: {e}")))?;

        let session = db
            .get_session(token)
            .map_err(|e| SessionError::Internal(format!("Database error: {e}")))?
            .ok_or(SessionError::NotFound)?;

        if session.status != "active" {
            return Err(SessionError::Closed);
        }

        let closed = db
            .close_session(token, creator_secret)
            .map_err(|e| SessionError::Internal(format!("Database error: {e}")))?;

        if !closed {
            return Err(SessionError::Forbidden);
        }

        // Wake connected clients so they can notify the user and close. `send`
        // errors only when nobody is listening, which is fine.
        if let Some(entry) = self.sessions.remove(token) {
            let _ = entry.shutdown.send(());
        }

        Ok(())
    }

    pub fn get_session(&self, token: &str) -> Result<crate::persistence::Session, SessionError> {
        let db = DocumentDB::new(&self.db_path)
            .map_err(|e| SessionError::Internal(format!("Failed to open database: {e}")))?;

        db.get_session(token)
            .map_err(|e| SessionError::Internal(format!("Database error: {e}")))?
            .ok_or(SessionError::NotFound)
    }

    pub async fn cleanup_idle_sessions(&mut self, timeout: Duration) {
        let now = Instant::now();
        let mut to_remove = Vec::new();

        for (token, entry) in &self.sessions {
            if now.duration_since(entry.last_active) > timeout {
                let server = entry.server.lock().await;
                if server.get_connected_clients().is_empty() {
                    to_remove.push(token.clone());
                }
            }
        }

        for token in to_remove {
            self.sessions.remove(&token);
        }
    }

    pub async fn cleanup_stale_clients(&mut self, timeout_secs: u64) {
        for entry in self.sessions.values() {
            entry
                .server
                .lock()
                .await
                .cleanup_stale_clients(timeout_secs);
        }
        if let Some(ref server) = self.default_server {
            server.lock().await.cleanup_stale_clients(timeout_secs);
        }
    }
}

#[derive(Debug)]
pub enum SessionError {
    NotFound,
    Closed,
    Forbidden,
    Internal(String),
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => write!(f, "Session not found"),
            Self::Closed => write!(f, "Session has ended"),
            Self::Forbidden => write!(f, "Invalid creator secret"),
            Self::Internal(msg) => write!(f, "Internal error: {msg}"),
        }
    }
}

fn generate_token() -> String {
    let bytes: [u8; 16] = rand::random();
    base64url_encode(&bytes)
}

fn base64url_encode(input: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

    let mut result = String::with_capacity((input.len() * 4 + 2) / 3);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;

        result.push(ALPHABET[((triple >> 18) & 0x3F) as usize] as char);
        result.push(ALPHABET[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(ALPHABET[((triple >> 6) & 0x3F) as usize] as char);
        }
        if chunk.len() > 2 {
            result.push(ALPHABET[(triple & 0x3F) as usize] as char);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_token_length_and_uniqueness() {
        let t1 = generate_token();
        let t2 = generate_token();
        assert_eq!(t1.len(), 22);
        assert_ne!(t1, t2);
    }

    #[test]
    fn test_token_is_url_safe() {
        for _ in 0..100 {
            let token = generate_token();
            assert!(token
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
        }
    }

    #[test]
    fn test_base64url_encode() {
        assert_eq!(base64url_encode(&[0, 0, 0]), "AAAA");
        assert_eq!(base64url_encode(&[255, 255, 255]), "____");
    }
}
