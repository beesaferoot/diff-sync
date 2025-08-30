use serde::{Deserialize, Serialize};
use std::fmt;

/// Represents a single edit operation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Edit {
    /// Insert text at position
    Insert { pos: usize, text: String },
    /// Delete text from position with length
    Delete { pos: usize, len: usize },
    /// Replace text at position
    Replace { pos: usize, old_len: usize, new_text: String },
}

/// A collection of edits that can be applied to transform one text into another
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditList {
    pub edits: Vec<Edit>,
    pub checksum: String,
}

impl EditList {
    pub fn new(edits: Vec<Edit>, source: &str) -> Self {
        Self {
            edits,
            checksum: checksum(source),
        }
    }

    pub fn empty(source: &str) -> Self {
        Self::new(Vec::new(), source)
    }

    pub fn is_empty(&self) -> bool {
        self.edits.is_empty()
    }

    pub fn len(&self) -> usize {
        self.edits.len()
    }
}

impl fmt::Display for EditList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.edits.is_empty() {
            write!(f, "No edits")
        } else {
            write!(f, "{} edits:", self.edits.len())?;
            for (i, edit) in self.edits.iter().enumerate() {
                write!(f, "\n  {}: {:?}", i + 1, edit)?;
            }
            Ok(())
        }
    }
}

/// Generate a simple checksum for a string
pub fn checksum(text: &str) -> String {
    format!("{:x}", text.len() ^ (text.chars().map(|c| c as u32).sum::<u32>() as usize))
}

/// Create a diff between two texts, returning the edits needed to transform `from` into `to`
/// Simplified implementation inspired by Google's diff-match-patch
pub fn diff(from: &str, to: &str) -> EditList {
    // Basic equality check (Fraser's optimization 1.1)
    if from == to {
        return EditList::empty(from);
    }

    // Handle empty cases
    if from.is_empty() {
        return EditList::new(vec![Edit::Insert { pos: 0, text: to.to_string() }], from);
    }
    if to.is_empty() {
        return EditList::new(vec![Edit::Delete { pos: 0, len: from.len() }], from);
    }

    // For now, use a simple approach: find the longest common substring
    // and create edits around it. This is less optimal but more reliable.
    
    // Simple prefix stripping
    let mut common_start = 0;
    let from_chars: Vec<char> = from.chars().collect();
    let to_chars: Vec<char> = to.chars().collect();
    
    while common_start < from_chars.len() && 
          common_start < to_chars.len() && 
          from_chars[common_start] == to_chars[common_start] {
        common_start += 1;
    }
    
    // Simple suffix stripping  
    let mut common_end = 0;
    while common_end < from_chars.len() - common_start && 
          common_end < to_chars.len() - common_start &&
          from_chars[from_chars.len() - 1 - common_end] == to_chars[to_chars.len() - 1 - common_end] {
        common_end += 1;
    }
    
    // Convert char positions back to byte positions
    let prefix_bytes = from_chars[..common_start].iter().map(|c| c.len_utf8()).sum::<usize>();
    let suffix_bytes = if common_end > 0 {
        from_chars[from_chars.len() - common_end..].iter().map(|c| c.len_utf8()).sum::<usize>()
    } else {
        0
    };
    
    let from_middle = &from[prefix_bytes..from.len() - suffix_bytes];
    let to_middle = &to[prefix_bytes..to.len() - suffix_bytes];
    
    let mut edits = Vec::new();
    
    // Simple replace operation for the middle section
    if !from_middle.is_empty() || !to_middle.is_empty() {
        if from_middle.is_empty() {
            edits.push(Edit::Insert {
                pos: prefix_bytes,
                text: to_middle.to_string(),
            });
        } else if to_middle.is_empty() {
            edits.push(Edit::Delete {
                pos: prefix_bytes,
                len: from_middle.len(),
            });
        } else {
            edits.push(Edit::Replace {
                pos: prefix_bytes,
                old_len: from_middle.len(),
                new_text: to_middle.to_string(),
            });
        }
    }

    EditList::new(edits, from)
}



/// Apply a list of edits to a text string, returning the result
/// This is a "fuzzy" patch that tries to apply edits even if the text has changed
pub fn patch(text: &str, edit_list: &EditList) -> Result<String, PatchError> {
    if edit_list.is_empty() {
        return Ok(text.to_string());
    }

    let mut result = text.to_string();

    // Apply edits in reverse order to avoid position shifting issues
    // This is simpler and more reliable than tracking offsets
    for edit in edit_list.edits.iter().rev() {
        match edit {
            Edit::Insert { pos, text: insert_text } => {
                let safe_pos = (*pos).min(result.len());
                result.insert_str(safe_pos, insert_text);
            }
            Edit::Delete { pos, len } => {
                let start_pos = (*pos).min(result.len());
                let end_pos = (start_pos + len).min(result.len());
                if start_pos < end_pos {
                    result.drain(start_pos..end_pos);
                }
            }
            Edit::Replace { pos, old_len, new_text } => {
                let start_pos = (*pos).min(result.len());
                let end_pos = (start_pos + old_len).min(result.len());
                result.replace_range(start_pos..end_pos, new_text);
            }
        }
    }

    Ok(result)
}

#[derive(Debug, Clone, PartialEq)]
pub enum PatchError {
    ChecksumMismatch,
    InvalidPosition,
    InvalidEdit,
}

impl fmt::Display for PatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PatchError::ChecksumMismatch => write!(f, "Checksum mismatch"),
            PatchError::InvalidPosition => write!(f, "Invalid position"),
            PatchError::InvalidEdit => write!(f, "Invalid edit"),
        }
    }
}

impl std::error::Error for PatchError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diff_and_patch() {
        let original = "The quick brown fox";
        let modified = "The quick red fox jumps";

        let edits = diff(original, modified);
        let result = patch(original, &edits).unwrap();

        assert_eq!(result, modified);
    }

    #[test]
    fn test_empty_diff() {
        let text = "Same text";
        let edits = diff(text, text);
        assert!(edits.is_empty());
    }

    #[test]
    fn test_fuzzy_patch() {
        let original = "Hello world";
        let modified = "Hello beautiful world";
        
        // Create a diff
        let edits = diff(original, modified);
        
        // Apply to slightly different text (fuzzy matching)
        let different_text = "Hello cruel world";
        let result = patch(different_text, &edits).unwrap();
        
        // Should still work reasonably well
        assert!(result.contains("beautiful"));
    }
}
