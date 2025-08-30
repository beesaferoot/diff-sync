use crate::Document;
use rusqlite::{Connection, Result as SqlResult, params};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Database manager for persistent document storage
pub struct DocumentDB {
    conn: Connection,
}

impl DocumentDB {
    /// Create or open database file
    pub fn new<P: AsRef<Path>>(db_path: P) -> SqlResult<Self> {
        let conn = Connection::open(db_path)?;
        let db = DocumentDB { conn };
        db.create_tables()?;
        Ok(db)
    }

    /// Create in-memory database for testing
    pub fn new_in_memory() -> SqlResult<Self> {
        let conn = Connection::open_in_memory()?;
        let db = DocumentDB { conn };
        db.create_tables()?;
        Ok(db)
    }

    /// Create the documents table
    fn create_tables(&self) -> SqlResult<()> {
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

        // Create default document if it doesn't exist
        self.create_default_document()?;
        Ok(())
    }

    /// Create the default collaborative document
    fn create_default_document(&self) -> SqlResult<()> {
        let now = current_timestamp();
        let default_content = "Welcome to collaborative editing with persistence!";
        
        self.conn.execute(
            "INSERT OR IGNORE INTO documents (name, content, version, created_at, updated_at)
             VALUES (?1, ?2, 0, ?3, ?3)",
            params!["main", default_content, now],
        )?;
        Ok(())
    }

    /// Load a document by name
    pub fn load_document(&self, name: &str) -> SqlResult<Option<Document>> {
        let mut stmt = self.conn.prepare(
            "SELECT content, version FROM documents WHERE name = ?1"
        )?;

        let mut rows = stmt.query_map([name], |row| {
            let content: String = row.get(0)?;
            let version: u64 = row.get(1)?;
            Ok(Document::new_with_version(content, version))
        })?;

        match rows.next() {
            Some(Ok(document)) => Ok(Some(document)),
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }
    }

    /// Save a document (insert or update)
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

    /// Update document content and increment version
    pub fn update_document(&self, name: &str, new_content: String) -> SqlResult<Document> {
        let now = current_timestamp();
        
        // Get current version
        let current_version: u64 = self.conn.query_row(
            "SELECT version FROM documents WHERE name = ?1",
            [name],
            |row| row.get(0),
        ).unwrap_or(0);

        let new_version = current_version + 1;

        // Update document
        self.conn.execute(
            "UPDATE documents SET content = ?1, version = ?2, updated_at = ?3 WHERE name = ?4",
            params![new_content, new_version, now, name],
        )?;

        Ok(Document::new_with_version(new_content, new_version))
    }

    /// List all documents
    pub fn list_documents(&self) -> SqlResult<Vec<(String, u64, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT name, version, created_at FROM documents ORDER BY updated_at DESC"
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,      // name
                row.get::<_, u64>(1)?,         // version  
                format_timestamp(row.get(2)?), // created_at
            ))
        })?;

        let mut documents = Vec::new();
        for row in rows {
            documents.push(row?);
        }
        Ok(documents)
    }

    /// Get document statistics
    pub fn get_stats(&self) -> SqlResult<DocumentStats> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM documents",
            [],
            |row| row.get(0),
        )?;

        let latest_update: Option<i64> = self.conn.query_row(
            "SELECT MAX(updated_at) FROM documents",
            [],
            |row| row.get(0),
        ).ok();

        Ok(DocumentStats {
            total_documents: count as u64,
            latest_update: latest_update.map(format_timestamp),
        })
    }
}

#[derive(Debug)]
pub struct DocumentStats {
    pub total_documents: u64,
    pub latest_update: Option<String>,
}

/// Get current Unix timestamp
fn current_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

/// Format Unix timestamp as human-readable string
fn format_timestamp(timestamp: i64) -> String {
    let datetime = UNIX_EPOCH + std::time::Duration::from_secs(timestamp as u64);
    format!("{:?}", datetime) // Simple debug format for now
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_persistence() {
        let db = DocumentDB::new_in_memory().unwrap();
        
        // Load default document
        let doc = db.load_document("main").unwrap().unwrap();
        assert_eq!(doc.content, "Welcome to collaborative editing with persistence!");
        assert_eq!(doc.version, 0);

        // Update document
        let updated = db.update_document("main", "Hello persistent world!".to_string()).unwrap();
        assert_eq!(updated.content, "Hello persistent world!");
        assert_eq!(updated.version, 1);

        // Verify persistence
        let reloaded = db.load_document("main").unwrap().unwrap();
        assert_eq!(reloaded.content, "Hello persistent world!");
        assert_eq!(reloaded.version, 1);
    }

    #[test]
    fn test_document_stats() {
        let db = DocumentDB::new_in_memory().unwrap();
        let stats = db.get_stats().unwrap();
        assert_eq!(stats.total_documents, 1); // Default document
    }
}
