use std::fs::File;
use std::io::{self, BufRead, BufReader};

use super::{Details, FileAnalyzer};
use crate::files::FileContext;

pub struct TextAnalyzer;

impl FileAnalyzer for TextAnalyzer {
    fn name(&self) -> &'static str {
        "Text"
    }

    fn supports(&self, file: &FileContext) -> bool {
        if !file.metadata.is_file() || file.sample.starts_with(b"\x7fELF") {
            return false;
        }
        // A sample can end partway through a valid UTF-8 character.
        let text = match std::str::from_utf8(&file.sample) {
            Ok(text) => text,
            Err(err) if err.error_len().is_none() => {
                std::str::from_utf8(&file.sample[..err.valid_up_to()]).unwrap_or("")
            }
            Err(_) => return false,
        };
        text.chars().all(is_text_character)
    }

    fn analyze(&self, file: &FileContext) -> io::Result<Details> {
        // Stream a line at a time instead of loading the entire file into memory.
        let mut reader = BufReader::new(File::open(&file.path)?);
        let mut line = String::new();
        let (mut lines, mut words, mut chars) = (0u64, 0u64, 0u64);
        let (mut lf, mut crlf, mut cr) = (0u64, 0u64, 0u64);
        let mut ascii = true;
        let mut trailing_newline = false;
        let mut interpreter = None;
        loop {
            line.clear();
            if reader.read_line(&mut line)? == 0 {
                break;
            }
            if !line.chars().all(is_text_character) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "non-text content after the initial sample; no text statistics reported",
                ));
            }
            if lines == 0 {
                interpreter = line.strip_prefix("#!").map(|s| s.trim().to_owned());
            }
            lines += 1;
            words += line.split_whitespace().count() as u64;
            chars += line.chars().count() as u64;
            ascii &= line.is_ascii();
            trailing_newline = line.ends_with('\n');
            if line.ends_with("\r\n") {
                crlf += 1;
            } else if trailing_newline {
                lf += 1;
            }
            cr += line.bytes().filter(|b| *b == b'\r').count() as u64;
        }
        let mut details = Details::new("Text");
        details.add(
            "Encoding",
            if ascii {
                "ASCII (UTF-8 compatible)"
            } else {
                "UTF-8"
            },
        );
        details.add("Lines", lines);
        details.add("Words", words);
        details.add("Characters", chars);
        let endings = match (lf > 0, crlf > 0, cr > crlf) {
            (false, false, false) => "None",
            (true, false, false) => "LF (Unix)",
            (false, true, false) => "CRLF (Windows)",
            (false, false, true) => "CR (not counted as a line separator)",
            _ => "Mixed",
        };
        details.add("Line endings", endings);
        details.add("Final newline", trailing_newline);
        if let Some(interpreter) = interpreter {
            details.add("Script interpreter", interpreter);
        }
        Ok(details)
    }
}

fn is_text_character(c: char) -> bool {
    !c.is_control() || matches!(c, '\n' | '\r' | '\t' | '\u{000c}')
}
