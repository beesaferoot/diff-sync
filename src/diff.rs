use serde::{Deserialize, Serialize};
use std::fmt;

/// A single edit operation with byte-offset positions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Edit {
    Insert {
        pos: usize,
        text: String,
    },
    Delete {
        pos: usize,
        len: usize,
    },
    Replace {
        pos: usize,
        old_len: usize,
        new_text: String,
    },
}

/// An ordered collection of edits with a checksum of the source text they were
/// computed against. Applying the edits to a string matching the checksum
/// produces the target text.
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
                write!(f, "\n  {}: {edit:?}", i + 1)?;
            }
            Ok(())
        }
    }
}

/// Simple checksum combining length and char-value sum. Not cryptographic —
/// used only for quick shadow-equality checks during sync.
pub fn checksum(text: &str) -> String {
    let hash = text.len() ^ (text.chars().map(|c| c as u32).sum::<u32>() as usize);
    format!("{hash:x}")
}

/// Compute the minimal edit list to transform `from` into `to`.
///
/// Strips common prefix and suffix, then emits a single Insert, Delete, or
/// Replace for the differing middle section. Positions are byte offsets.
pub fn diff(from: &str, to: &str) -> EditList {
    if from == to {
        return EditList::empty(from);
    }
    if from.is_empty() {
        return EditList::new(
            vec![Edit::Insert {
                pos: 0,
                text: to.to_string(),
            }],
            from,
        );
    }
    if to.is_empty() {
        return EditList::new(
            vec![Edit::Delete {
                pos: 0,
                len: from.len(),
            }],
            from,
        );
    }

    let from_chars: Vec<char> = from.chars().collect();
    let to_chars: Vec<char> = to.chars().collect();

    let mut common_start = 0;
    while common_start < from_chars.len()
        && common_start < to_chars.len()
        && from_chars[common_start] == to_chars[common_start]
    {
        common_start += 1;
    }

    let mut common_end = 0;
    while common_end < from_chars.len() - common_start
        && common_end < to_chars.len() - common_start
        && from_chars[from_chars.len() - 1 - common_end]
            == to_chars[to_chars.len() - 1 - common_end]
    {
        common_end += 1;
    }

    let prefix_bytes: usize = from_chars[..common_start]
        .iter()
        .map(|c| c.len_utf8())
        .sum();
    let suffix_bytes: usize = if common_end > 0 {
        from_chars[from_chars.len() - common_end..]
            .iter()
            .map(|c| c.len_utf8())
            .sum()
    } else {
        0
    };

    let from_middle = &from[prefix_bytes..from.len() - suffix_bytes];
    let to_middle = &to[prefix_bytes..to.len() - suffix_bytes];

    let edit = match (from_middle.is_empty(), to_middle.is_empty()) {
        (true, true) => return EditList::empty(from),
        (true, false) => Edit::Insert {
            pos: prefix_bytes,
            text: to_middle.to_string(),
        },
        (false, true) => Edit::Delete {
            pos: prefix_bytes,
            len: from_middle.len(),
        },
        (false, false) => Edit::Replace {
            pos: prefix_bytes,
            old_len: from_middle.len(),
            new_text: to_middle.to_string(),
        },
    };

    EditList::new(vec![edit], from)
}

/// Apply edits to `text`, returning the transformed result.
///
/// Edits are applied in reverse order to avoid cascading position shifts.
/// Positions are clamped to text bounds for fuzzy-patch tolerance.
pub fn patch(text: &str, edit_list: &EditList) -> Result<String, PatchError> {
    if edit_list.is_empty() {
        return Ok(text.to_string());
    }

    let mut result = text.to_string();

    for edit in edit_list.edits.iter().rev() {
        match edit {
            Edit::Insert {
                pos,
                text: insert_text,
            } => {
                let safe_pos = (*pos).min(result.len());
                result.insert_str(safe_pos, insert_text);
            }
            Edit::Delete { pos, len } => {
                let start = (*pos).min(result.len());
                let end = (start + len).min(result.len());
                if start < end {
                    result.drain(start..end);
                }
            }
            Edit::Replace {
                pos,
                old_len,
                new_text,
            } => {
                let start = (*pos).min(result.len());
                let end = (start + old_len).min(result.len());
                result.replace_range(start..end, new_text);
            }
        }
    }

    Ok(result)
}

/// Errors that can occur during patch application.
#[derive(Debug, Clone, PartialEq)]
pub enum PatchError {
    ChecksumMismatch,
    InvalidPosition,
    InvalidEdit,
}

impl fmt::Display for PatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ChecksumMismatch => write!(f, "Checksum mismatch"),
            Self::InvalidPosition => write!(f, "Invalid position"),
            Self::InvalidEdit => write!(f, "Invalid edit"),
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
        assert!(diff(text, text).is_empty());
    }

    #[test]
    fn test_fuzzy_patch() {
        let original = "Hello world";
        let modified = "Hello beautiful world";
        let edits = diff(original, modified);

        let different_text = "Hello cruel world";
        let result = patch(different_text, &edits).unwrap();
        assert!(result.contains("beautiful"));
    }
}
