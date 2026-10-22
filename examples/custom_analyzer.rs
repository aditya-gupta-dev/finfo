//! Run with: cargo run --example custom_analyzer -- src/files.rs

use std::fs::File;
use std::io::{self, BufRead, BufReader};

use finfo::files::analyzers::{Details, FileAnalyzer, default_analyzers};
use finfo::files::{FileContext, inspect_with};

struct TodoAnalyzer;

impl FileAnalyzer for TodoAnalyzer {
    fn name(&self) -> &'static str {
        "TODO markers"
    }

    fn supports(&self, file: &FileContext) -> bool {
        file.metadata.is_file() && file.path.extension().is_some_and(|ext| ext == "rs")
    }

    fn analyze(&self, file: &FileContext) -> io::Result<Details> {
        let mut count = 0;
        for line in BufReader::new(File::open(&file.path)?).lines() {
            if line?.contains("TODO") {
                count += 1;
            }
        }
        let mut details = Details::new(self.name());
        details.add("Lines containing TODO", count);
        Ok(details)
    }
}

fn main() -> io::Result<()> {
    let mut analyzers = default_analyzers();
    analyzers.push(Box::new(TodoAnalyzer));
    for path in std::env::args_os().skip(1) {
        println!("{}", inspect_with(path, &analyzers)?);
    }
    Ok(())
}
