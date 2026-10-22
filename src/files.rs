//! File inspection, independent of how type-specific details are collected.

pub mod analyzers;

use std::fmt;
use std::fs::{self, File, Metadata};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use analyzers::{Details, FileAnalyzer};

/// Shared input for analyzers. Symlinks are inspected without following them.
pub struct FileContext {
    pub path: PathBuf,
    pub metadata: Metadata,
    /// At most 8 KiB of a regular file; empty for other filesystem entries.
    pub sample: Vec<u8>,
}

impl FileContext {
    fn new(path: &Path) -> io::Result<Self> {
        let metadata = fs::symlink_metadata(path)?;
        let mut sample = Vec::new();
        if metadata.is_file() {
            File::open(path)?.take(8192).read_to_end(&mut sample)?;
        }
        Ok(Self {
            path: std::path::absolute(path)?,
            metadata,
            sample,
        })
    }
}

/// Structured output. Analyzers supply data; Display owns all formatting.
pub struct FileInfo {
    pub path: PathBuf,
    pub summary: Details,
    pub details: Vec<Details>,
    /// A failed analyzer does not hide the common metadata or other sections.
    pub warnings: Vec<String>,
}

pub fn get_file_info(path: impl AsRef<Path>) -> io::Result<FileInfo> {
    inspect_with(path, &analyzers::default_analyzers())
}

/// Every matching analyzer runs, allowing additional details for an existing type.
pub fn inspect_with(
    path: impl AsRef<Path>,
    analyzers: &[Box<dyn FileAnalyzer>],
) -> io::Result<FileInfo> {
    let context = FileContext::new(path.as_ref())?;
    let metadata = &context.metadata;
    let mut summary = Details::new("General");
    summary.add("Type", entity_type(metadata));
    summary.add("Absolute path", context.path.display());
    summary.add("Size", format_size(metadata.len()));
    summary.add("Read-only", metadata.permissions().readonly());
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let mode = metadata.permissions().mode();
        summary.add("Permissions", format!("{:04o}", mode & 0o7777));
        summary.add(
            "Owner UID / GID",
            format!("{} / {}", metadata.uid(), metadata.gid()),
        );
        summary.add("Hard links", metadata.nlink());
        if metadata.is_file() {
            summary.add("Executable bits", mode & 0o111 != 0);
        }
    }

    let mut info = FileInfo {
        path: context.path.clone(),
        summary,
        details: Vec::new(),
        warnings: Vec::new(),
    };
    for analyzer in analyzers {
        if analyzer.supports(&context) {
            match analyzer.analyze(&context) {
                Ok(details) => info.details.push(details),
                Err(err) => info.warnings.push(format!("{}: {err}", analyzer.name())),
            }
        }
    }
    if metadata.is_file() && info.details.is_empty() && info.warnings.is_empty() {
        let mut details = Details::new("Content");
        details.add("Format", "Binary / unrecognized encoding or format");
        info.details.push(details);
    }
    Ok(info)
}

impl fmt::Display for FileInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "File: {}", visible(&self.path.to_string_lossy()))?;
        let width = std::iter::once(&self.summary)
            .chain(&self.details)
            .flat_map(|section| &section.fields)
            .map(|(label, _)| label.chars().count())
            .max()
            .unwrap_or(0);
        for section in std::iter::once(&self.summary).chain(&self.details) {
            writeln!(f, "  {}", visible(&section.title))?;
            for (label, value) in &section.fields {
                // Embedded newlines are intentional, e.g. one library per row.
                for (index, line) in value.split('\n').enumerate() {
                    let label = if index == 0 { label.as_str() } else { "" };
                    writeln!(f, "    {:width$} : {}", visible(label), visible(line))?;
                }
            }
        }
        for warning in &self.warnings {
            writeln!(f, "  Warning: {}", visible(warning))?;
        }
        Ok(())
    }
}

// Avoid interpreting terminal control characters from filenames or file content.
fn visible(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    for c in value.chars() {
        if c.is_control() {
            result.extend(c.escape_default());
        } else {
            result.push(c);
        }
    }
    result
}

pub fn run(files: &[impl AsRef<Path>]) -> io::Result<()> {
    let mut failed = 0;
    for file in files {
        match get_file_info(file) {
            Ok(info) => {
                if !info.warnings.is_empty() {
                    failed += 1;
                }
                println!("{info}");
            }
            Err(err) => {
                failed += 1;
                eprintln!("{}: {err}", visible(&file.as_ref().to_string_lossy()));
            }
        }
    }
    if failed == 0 {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "Could not fully inspect {failed} file(s)."
        )))
    }
}

pub(super) fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{size:.2} {} ({bytes} bytes)", UNITS[unit])
    }
}

pub(super) fn entity_type(metadata: &Metadata) -> &'static str {
    let kind = metadata.file_type();
    if kind.is_symlink() {
        "Symbolic link"
    } else if kind.is_dir() {
        "Directory"
    } else if kind.is_file() {
        "Regular file"
    } else {
        #[cfg(unix)]
        {
            use std::os::unix::fs::FileTypeExt;
            if kind.is_fifo() {
                return "Named pipe";
            }
            if kind.is_socket() {
                return "Socket";
            }
            if kind.is_block_device() {
                return "Block device";
            }
            if kind.is_char_device() {
                return "Character device";
            }
        }
        "Special file"
    }
}
