use std::path::Path;

use crate::files::analyzers::{Details, FileAnalyzer};

pub struct MP4Analyzer;

fn is_mp4_file(file_path: &Path) -> bool {
    file_path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("mp4"))
        .unwrap_or(false)
}

impl FileAnalyzer for MP4Analyzer {
    fn name(&self) -> &'static str {
        "MP4"
    }

    fn analyze(&self, file: &crate::files::FileContext) -> std::io::Result<super::Details> {
        let path_str = file.path.to_str().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid UTF-8 in path")
        })?;

        let metadata = revelo::Metadata::from_file(path_str).unwrap();
        let mut details = Details::new("Video Details");

        for (key, value) in metadata.general() {
            details.add(key, value);
        }

        return Ok(details);
    }

    fn supports(&self, file: &crate::files::FileContext) -> bool {
        file.metadata.is_file() && is_mp4_file(&file.path)
    }
}
