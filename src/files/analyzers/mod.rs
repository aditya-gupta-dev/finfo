//! Add an analyzer here and register it in `default_analyzers` to extend the CLI.

mod elf;
mod filesystem;
mod mp4;
mod text;

use std::fmt::Display;
use std::io;

use super::FileContext;

/// An analyzer returns labeled fields and never prints directly.
pub trait FileAnalyzer {
    fn name(&self) -> &'static str;
    fn supports(&self, file: &FileContext) -> bool;
    fn analyze(&self, file: &FileContext) -> io::Result<Details>;
}

#[derive(Debug)]
pub struct Details {
    pub title: String,
    pub fields: Vec<(String, String)>,
}

impl Details {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            fields: Vec::new(),
        }
    }

    pub fn add(&mut self, label: impl Into<String>, value: impl Display) {
        self.fields.push((label.into(), value.to_string()));
    }
}

pub fn default_analyzers() -> Vec<Box<dyn FileAnalyzer>> {
    vec![
        Box::new(text::TextAnalyzer),
        Box::new(elf::ElfAnalyzer),
        Box::new(filesystem::DirectoryAnalyzer),
        Box::new(filesystem::SymlinkAnalyzer),
        Box::new(mp4::MP4Analyzer),
    ]
}
