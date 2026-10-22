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

fn format_duration<'a>(ms: &'a str) -> String {
    let ms = ms.parse::<u64>().unwrap();
    let total_seconds = ms / 1000;
    let millis = ms % 1000;
    let seconds = total_seconds % 60;
    let minutes = (total_seconds / 60) % 60;
    let hours = total_seconds / 3600;

    if hours > 0 {
        format!("{hours:02} hrs {minutes:02} mins {seconds:02}.{millis:03} sec")
    } else {
        format!("{minutes:02} mins {seconds:02}.{millis:03} sec")
    }
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
            if key == "Duration" {
                details.add(key, format_duration(value));
                continue;
            }
            details.add(key, value);
        }

        for (key, value) in metadata.video() {
            details.add(key, value);
        }

        return Ok(details);
    }

    fn supports(&self, file: &crate::files::FileContext) -> bool {
        file.metadata.is_file() && is_mp4_file(&file.path)
    }
}
