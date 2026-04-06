use crate::Document;
use rusqlite::{params, Connection, Result as SqlResult};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub token: String,
    pub document_name: String,
    pub status: String,
    pub created_at: i64,
    pub closed_at: Option<i64>,
}

const DEFAULT_CONTENT: &str = "Welcome to collaborative editing with persistence!";

/// SQLite-backed persistent storage for documents.
pub struct DocumentDB {
    conn: Connection,
}

impl DocumentDB {
    pub fn new<P: AsRef<Path>>(db_path: P) -> SqlResult<Self> {
        let conn = Connection::open(db_path)?;
        let db = Self { conn };
        db.init_schema()?;
        Ok(db)
    }

    pub fn new_in_memory() -> SqlResult<Self> {
        let conn = Connection::open_in_memory()?;
        let db = Self { conn };
        db.init_schema()?;
        Ok(db)
    }

    fn init_schema(&self) -> SqlResult<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS documents (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT UNIQUE NOT NULL,
                content TEXT NOT NULL,
                version INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            )",
            [],
        )?;

        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS sessions (
                token TEXT PRIMARY KEY,
                creator_secret TEXT NOT NULL,
                document_name TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'active',
                created_at INTEGER NOT NULL,
                closed_at INTEGER
            )",
            [],
        )?;

        let now = current_timestamp();
        self.conn.execute(
            "INSERT OR IGNORE INTO documents (name, content, version, created_at, updated_at)
             VALUES (?1, ?2, 0, ?3, ?3)",
            params!["main", DEFAULT_CONTENT, now],
        )?;
        Ok(())
    }

    pub fn load_document(&self, name: &str) -> SqlResult<Option<Document>> {
        let mut stmt = self
            .conn
            .prepare("SELECT content, version FROM documents WHERE name = ?1")?;

        let mut rows = stmt.query_map([name], |row| {
            Ok(Document::new_with_version(row.get(0)?, row.get(1)?))
        })?;

        match rows.next() {
            Some(result) => Ok(Some(result?)),
            None => Ok(None),
        }
    }

    pub fn save_document(&self, name: &str, document: &Document) -> SqlResult<()> {
        let now = current_timestamp();
        self.conn.execute(
            "INSERT OR REPLACE INTO documents (name, content, version, created_at, updated_at)
             VALUES (?1, ?2, ?3,
                     COALESCE((SELECT created_at FROM documents WHERE name = ?1), ?4),
                     ?4)",
            params![name, document.content, document.version, now],
        )?;
        Ok(())
    }

    pub fn update_document(&self, name: &str, new_content: String) -> SqlResult<Document> {
        let current_version: u64 = self
            .conn
            .query_row(
                "SELECT version FROM documents WHERE name = ?1",
                [name],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let new_version = current_version + 1;
        let now = current_timestamp();

        self.conn.execute(
            "UPDATE documents SET content = ?1, version = ?2, updated_at = ?3 WHERE name = ?4",
            params![new_content, new_version, now, name],
        )?;

        Ok(Document::new_with_version(new_content, new_version))
    }

    pub fn list_documents(&self) -> SqlResult<Vec<(String, u64, String)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT name, version, created_at FROM documents ORDER BY updated_at DESC")?;

        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, u64>(1)?,
                format_timestamp(row.get(2)?),
            ))
        })?;

        rows.collect()
    }

    pub fn create_session(
        &self,
        token: &str,
        creator_secret: &str,
        initial_content: &str,
    ) -> SqlResult<()> {
        let now = current_timestamp();
        let document_name = format!("session_{token}");

        self.conn.execute(
            "INSERT INTO documents (name, content, version, created_at, updated_at)
             VALUES (?1, ?2, 0, ?3, ?3)",
            params![document_name, initial_content, now],
        )?;

        self.conn.execute(
            "INSERT INTO sessions (token, creator_secret, document_name, status, created_at)
             VALUES (?1, ?2, ?3, 'active', ?4)",
            params![token, creator_secret, document_name, now],
        )?;

        Ok(())
    }

    pub fn get_session(&self, token: &str) -> SqlResult<Option<Session>> {
        let mut stmt = self.conn.prepare(
            "SELECT token, document_name, status, created_at, closed_at
             FROM sessions WHERE token = ?1",
        )?;

        let mut rows = stmt.query_map([token], |row| {
            Ok(Session {
                token: row.get(0)?,
                document_name: row.get(1)?,
                status: row.get(2)?,
                created_at: row.get(3)?,
                closed_at: row.get(4)?,
            })
        })?;

        match rows.next() {
            Some(result) => Ok(Some(result?)),
            None => Ok(None),
        }
    }

    pub fn close_session(&self, token: &str, creator_secret: &str) -> SqlResult<bool> {
        let now = current_timestamp();
        let rows_updated = self.conn.execute(
            "UPDATE sessions SET status = 'closed', closed_at = ?1
             WHERE token = ?2 AND creator_secret = ?3 AND status = 'active'",
            params![now, token, creator_secret],
        )?;
        Ok(rows_updated > 0)
    }

    pub fn is_session_active(&self, token: &str) -> SqlResult<bool> {
        let mut stmt = self
            .conn
            .prepare("SELECT 1 FROM sessions WHERE token = ?1 AND status = 'active'")?;
        let exists = stmt.exists([token])?;
        Ok(exists)
    }

    pub fn get_stats(&self) -> SqlResult<DocumentStats> {
        let count: u64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM documents", [], |row| row.get(0))?;

        let latest_update: Option<i64> = self
            .conn
            .query_row("SELECT MAX(updated_at) FROM documents", [], |row| {
                row.get(0)
            })
            .ok();

        Ok(DocumentStats {
            total_documents: count,
            latest_update: latest_update.map(format_timestamp),
        })
    }
}

#[derive(Debug)]
pub struct DocumentStats {
    pub total_documents: u64,
    pub latest_update: Option<String>,
}

fn current_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX epoch")
        .as_secs() as i64
}

fn format_timestamp(timestamp: i64) -> String {
    let datetime = UNIX_EPOCH + std::time::Duration::from_secs(timestamp as u64);
    format!("{datetime:?}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_persistence() {
        let db = DocumentDB::new_in_memory().unwrap();

        let doc = db.load_document("main").unwrap().unwrap();
        assert_eq!(doc.content, DEFAULT_CONTENT);
        assert_eq!(doc.version, 0);

        let updated = db
            .update_document("main", "Hello persistent world!".to_string())
            .unwrap();
        assert_eq!(updated.content, "Hello persistent world!");
        assert_eq!(updated.version, 1);

        let reloaded = db.load_document("main").unwrap().unwrap();
        assert_eq!(reloaded.content, "Hello persistent world!");
        assert_eq!(reloaded.version, 1);
    }

    #[test]
    fn test_document_stats() {
        let db = DocumentDB::new_in_memory().unwrap();
        let stats = db.get_stats().unwrap();
        assert_eq!(stats.total_documents, 1);
    }

    #[test]
    fn test_create_and_get_session() {
        let db = DocumentDB::new_in_memory().unwrap();
        db.create_session("tok123", "secret456", "hello").unwrap();

        let session = db.get_session("tok123").unwrap().unwrap();
        assert_eq!(session.token, "tok123");
        assert_eq!(session.document_name, "session_tok123");
        assert_eq!(session.status, "active");
        assert!(session.closed_at.is_none());

        let doc = db.load_document("session_tok123").unwrap().unwrap();
        assert_eq!(doc.content, "hello");
    }

    #[test]
    fn test_close_session() {
        let db = DocumentDB::new_in_memory().unwrap();
        db.create_session("tok1", "secret1", "").unwrap();

        assert!(db.is_session_active("tok1").unwrap());

        assert!(!db.close_session("tok1", "wrong").unwrap());
        assert!(db.is_session_active("tok1").unwrap());

        assert!(db.close_session("tok1", "secret1").unwrap());
        assert!(!db.is_session_active("tok1").unwrap());

        let session = db.get_session("tok1").unwrap().unwrap();
        assert_eq!(session.status, "closed");
        assert!(session.closed_at.is_some());
    }

    #[test]
    fn test_get_nonexistent_session() {
        let db = DocumentDB::new_in_memory().unwrap();
        assert!(db.get_session("nope").unwrap().is_none());
        assert!(!db.is_session_active("nope").unwrap());
    }
}
