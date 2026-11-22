use super::{Segment, SegmentData};
use crate::config::{InputData, SegmentId};
use std::collections::HashMap;

#[derive(Default)]
pub struct DirectorySegment;

impl DirectorySegment {
    pub fn new() -> Self {
        Self
    }

    /// Extract directory name from path, handling both Unix and Windows separators
    fn extract_directory_name(path: &str) -> String {
        let trimmed = path.trim_end_matches(['/', '\\']);
        if trimmed.is_empty() {
            return "root".to_string();
        }

        let name = trimmed.rsplit(['/', '\\']).next().unwrap_or(trimmed);
        if name.is_empty() {
            "root".to_string()
        } else {
            name.to_string()
        }
    }
}

impl Segment for DirectorySegment {
    fn collect(&self, input: &InputData) -> Option<SegmentData> {
        let current_dir = input.current_dir();
        if current_dir.is_empty() {
            return None;
        }

        // Handle cross-platform path separators manually for better compatibility
        let dir_name = Self::extract_directory_name(current_dir);

        // Store the full path in metadata for potential use
        let mut metadata = HashMap::new();
        metadata.insert("full_path".to_string(), current_dir.to_string());

        Some(SegmentData {
            primary: dir_name,
            secondary: String::new(),
            metadata,
        })
    }

    fn id(&self) -> SegmentId {
        SegmentId::Directory
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_directory_name_across_platforms() {
        assert_eq!(
            DirectorySegment::extract_directory_name("/Users/me/project"),
            "project"
        );
        assert_eq!(
            DirectorySegment::extract_directory_name("/Users/me/project/"),
            "project"
        );
        assert_eq!(
            DirectorySegment::extract_directory_name("/Users/me/project///"),
            "project"
        );
        assert_eq!(DirectorySegment::extract_directory_name("/"), "root");
        assert_eq!(
            DirectorySegment::extract_directory_name(r"C:\Users\me\repo"),
            "repo"
        );
        assert_eq!(
            DirectorySegment::extract_directory_name(r"C:\Users\me\repo\"),
            "repo"
        );
        assert_eq!(
            DirectorySegment::extract_directory_name(r"C:\Users\me/repo/"),
            "repo"
        );
        assert_eq!(DirectorySegment::extract_directory_name("repo"), "repo");
    }
}
