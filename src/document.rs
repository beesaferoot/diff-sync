use serde::{Deserialize, Serialize};
use std::fmt;

/// Represents the content of a document
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub content: String,
    pub version: u64,
}

impl Document {
    pub fn new(content: String) -> Self {
        Self {
            content,
            version: 0,
        }
    }

    pub fn new_with_version(content: String, version: u64) -> Self {
        Self {
            content,
            version,
        }
    }

    pub fn update(&mut self, new_content: String) {
        self.content = new_content;
        self.version += 1;
    }

    pub fn len(&self) -> usize {
        self.content.len()
    }

    pub fn is_empty(&self) -> bool {
        self.content.is_empty()
    }
}

impl fmt::Display for Document {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (v{})", self.content, self.version)
    }
}

impl From<&str> for Document {
    fn from(content: &str) -> Self {
        Self::new(content.to_string())
    }
}

impl From<String> for Document {
    fn from(content: String) -> Self {
        Self::new(content)
    }
}
