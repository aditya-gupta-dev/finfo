use std::fs;
use std::io;

use super::{Details, FileAnalyzer};
use crate::files::{FileContext, entity_type, format_size};

pub struct DirectoryAnalyzer;

impl FileAnalyzer for DirectoryAnalyzer {
    fn name(&self) -> &'static str {
        "Directory"
    }

    fn supports(&self, file: &FileContext) -> bool {
        file.metadata.is_dir()
    }

    fn analyze(&self, file: &FileContext) -> io::Result<Details> {
        let (mut files, mut dirs, mut links, mut others, mut bytes) =
            (0u64, 0u64, 0u64, 0u64, 0u64);
        for entry in fs::read_dir(&file.path)? {
            let metadata = fs::symlink_metadata(entry?.path())?;
            if metadata.is_symlink() {
                links += 1;
            } else if metadata.is_dir() {
                dirs += 1;
            } else if metadata.is_file() {
                files += 1;
                bytes += metadata.len();
            } else {
                others += 1;
            }
        }
        let mut details = Details::new("Directory (immediate children)");
        details.add("Entries", files + dirs + links + others);
        details.add("Files", files);
        details.add("Directories", dirs);
        details.add("Symbolic links", links);
        details.add("Special entries", others);
        details.add("File sizes combined", format_size(bytes));
        Ok(details)
    }
}

pub struct SymlinkAnalyzer;

impl FileAnalyzer for SymlinkAnalyzer {
    fn name(&self) -> &'static str {
        "Symbolic link"
    }

    fn supports(&self, file: &FileContext) -> bool {
        file.metadata.is_symlink()
    }

    fn analyze(&self, file: &FileContext) -> io::Result<Details> {
        let mut details = Details::new("Symbolic link");
        details.add("Target", fs::read_link(&file.path)?.display());
        match fs::canonicalize(&file.path) {
            Ok(target) => {
                details.add("Resolved target", target.display());
                details.add("Target type", entity_type(&fs::metadata(target)?));
            }
            Err(err) => {
                details.add("Target status", format!("Unresolved: {err}"));
            }
        }
        Ok(details)
    }
}
