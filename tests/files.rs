use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use finfo::files::analyzers::{Details, FileAnalyzer, default_analyzers};
use finfo::files::{FileContext, FileInfo, get_file_info, inspect_with};

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "finfo-tests-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn write(&self, name: &str, bytes: impl AsRef<[u8]>) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, bytes).unwrap();
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn field<'a>(info: &'a FileInfo, name: &str) -> &'a str {
    std::iter::once(&info.summary)
        .chain(&info.details)
        .flat_map(|section| &section.fields)
        .find(|(label, _)| label == name)
        .unwrap_or_else(|| panic!("missing field {name} in {info}"))
        .1
        .as_str()
}

#[test]
fn text_statistics_cover_empty_unicode_and_line_endings() {
    let scratch = Scratch::new();
    let cases = [
        ("", "0", "0", "0", "None", "false"),
        ("hello world\n", "1", "2", "12", "LF (Unix)", "true"),
        (
            "héllo 世界\r\nlast",
            "2",
            "3",
            "14",
            "CRLF (Windows)",
            "false",
        ),
        ("a\r\nb\nc", "3", "3", "6", "Mixed", "false"),
        ("\n\n", "2", "0", "2", "LF (Unix)", "true"),
    ];
    for (text, lines, words, characters, endings, final_newline) in cases {
        let info = get_file_info(scratch.write("text", text)).unwrap();
        assert!(info.warnings.is_empty(), "{info}");
        assert_eq!(field(&info, "Lines"), lines);
        assert_eq!(field(&info, "Words"), words);
        assert_eq!(field(&info, "Characters"), characters);
        assert_eq!(field(&info, "Line endings"), endings);
        assert_eq!(field(&info, "Final newline"), final_newline);
    }
}

#[test]
fn detects_content_without_extensions_and_handles_utf8_sample_boundary() {
    let scratch = Scratch::new();
    let text = format!("{}é\n", "a".repeat(8191));
    let info = get_file_info(scratch.write("extensionless", text)).unwrap();
    assert!(info.warnings.is_empty());
    assert_eq!(field(&info, "Encoding"), "UTF-8");
    assert_eq!(field(&info, "Characters"), "8193");
    let script = get_file_info(scratch.write("script", "#!/usr/bin/env sh\necho hello\n")).unwrap();
    assert_eq!(field(&script, "Script interpreter"), "/usr/bin/env sh");
    assert_eq!(field(&script, "Lines"), "2");
}

#[test]
fn binary_content_does_not_get_text_statistics() {
    let scratch = Scratch::new();
    for bytes in [b"abc\0def".as_slice(), b"\xff\xfea\0", b"\x89PNG\r\n\x1a\n"] {
        let info = get_file_info(scratch.write("fake.txt", bytes)).unwrap();
        assert!(field(&info, "Format").starts_with("Binary"));
        assert!(info.warnings.is_empty());
    }
    let mut bytes = vec![b'a'; 9000];
    bytes.push(0xff);
    let info = get_file_info(scratch.write("late-binary", bytes)).unwrap();
    assert_eq!(info.warnings.len(), 1);
    assert!(
        info.details.is_empty(),
        "partial statistics must not be returned"
    );
}

#[cfg(unix)]
#[test]
fn directories_and_links_are_not_followed_or_recursed() {
    use std::os::unix::fs::symlink;
    let scratch = Scratch::new();
    scratch.write("file", "abc");
    fs::create_dir(scratch.0.join("nested")).unwrap();
    fs::write(scratch.0.join("nested/child"), "not counted").unwrap();
    symlink("file", scratch.0.join("link")).unwrap();
    symlink("missing", scratch.0.join("dangling")).unwrap();
    let dir = get_file_info(&scratch.0).unwrap();
    assert_eq!(field(&dir, "Entries"), "4");
    assert_eq!(field(&dir, "Files"), "1");
    assert_eq!(field(&dir, "Directories"), "1");
    assert_eq!(field(&dir, "Symbolic links"), "2");
    assert_eq!(field(&dir, "File sizes combined"), "3 B");
    let link = get_file_info(scratch.0.join("link")).unwrap();
    assert_eq!(field(&link, "Type"), "Symbolic link");
    assert_eq!(field(&link, "Target"), "file");
    assert_eq!(field(&link, "Target type"), "Regular file");
    assert!(!link.to_string().contains("  Text\n"));
    let dangling = get_file_info(scratch.0.join("dangling")).unwrap();
    assert_eq!(field(&dangling, "Type"), "Symbolic link");
    assert!(field(&dangling, "Target status").starts_with("Unresolved:"));
    symlink("loop", scratch.0.join("loop")).unwrap();
    let loop_info = get_file_info(scratch.0.join("loop")).unwrap();
    assert!(field(&loop_info, "Target status").starts_with("Unresolved:"));
}

#[cfg(unix)]
#[test]
fn special_files_are_inspected_without_opening_their_contents() {
    use std::os::unix::net::UnixListener;
    let scratch = Scratch::new();
    let path = scratch.0.join("socket");
    let _listener = UnixListener::bind(&path).unwrap();
    let info = get_file_info(path).unwrap();
    assert_eq!(field(&info, "Type"), "Socket");
    assert!(info.details.is_empty());
}

struct ExtraAnalyzer {
    fail: bool,
}

impl FileAnalyzer for ExtraAnalyzer {
    fn name(&self) -> &'static str {
        "Extra"
    }
    fn supports(&self, file: &FileContext) -> bool {
        file.metadata.is_file()
    }
    fn analyze(&self, _: &FileContext) -> io::Result<Details> {
        if self.fail {
            return Err(io::Error::other("example failure"));
        }
        let mut details = Details::new("Extra");
        details.add("Custom value", "first\nsecond");
        Ok(details)
    }
}

#[test]
fn custom_analyzers_compose_and_fail_independently() {
    let scratch = Scratch::new();
    let path = scratch.write("text", "hello\n");
    let mut analyzers = default_analyzers();
    analyzers.push(Box::new(ExtraAnalyzer { fail: true }));
    analyzers.push(Box::new(ExtraAnalyzer { fail: false }));
    let info = inspect_with(path, &analyzers).unwrap();
    assert_eq!(field(&info, "Lines"), "1");
    assert_eq!(field(&info, "Custom value"), "first\nsecond");
    assert_eq!(info.warnings, ["Extra: example failure"]);
    assert!(info.to_string().contains("                  : second"));
}

// Minimal valid metadata-only ELF fixtures; no executable code or section headers.
fn put(bytes: &mut [u8], offset: usize, width: usize, value: u64, little: bool) {
    let encoded = if little {
        value.to_le_bytes()
    } else {
        value.to_be_bytes()
    };
    let encoded = if little {
        &encoded[..width]
    } else {
        &encoded[8 - width..]
    };
    bytes[offset..offset + width].copy_from_slice(encoded);
}

fn elf_fixture(wide: bool, little: bool) -> Vec<u8> {
    let mut bytes = vec![0; 512];
    let word = if wide { 8 } else { 4 };
    let header = if wide { 64 } else { 52 };
    let phsize = if wide { 56 } else { 32 };
    bytes[..4].copy_from_slice(b"\x7fELF");
    bytes[4] = if wide { 2 } else { 1 };
    bytes[5] = if little { 1 } else { 2 };
    bytes[6] = 1;
    put(&mut bytes, 16, 2, 3, little);
    put(&mut bytes, 18, 2, if wide { 62 } else { 3 }, little);
    put(&mut bytes, 20, 4, 1, little);
    put(&mut bytes, 24, word, 0x400100, little);
    put(&mut bytes, if wide { 32 } else { 28 }, word, header, little);
    let sizes = if wide { 52 } else { 40 };
    put(&mut bytes, sizes, 2, header, little);
    put(&mut bytes, sizes + 2, 2, phsize, little);
    put(&mut bytes, sizes + 4, 2, 3, little);
    let strings = b"\0libalpha.so\0libbeta.so\0sample.so\0/opt/lib\0";
    bytes[384..384 + strings.len()].copy_from_slice(strings);
    let interpreter = b"/lib/test-loader\0";
    bytes[448..448 + interpreter.len()].copy_from_slice(interpreter);
    let soname = strings.windows(9).position(|s| s == b"sample.so").unwrap();
    let runpath = strings.windows(8).position(|s| s == b"/opt/lib").unwrap();
    let entries = [
        (5, 0x400180),
        (10, strings.len() as u64),
        (1, 1),
        (1, 13),
        (14, soname as u64),
        (29, runpath as u64),
        (0, 0),
    ];
    for (index, (tag, value)) in entries.into_iter().enumerate() {
        put(&mut bytes, 256 + index * word * 2, word, tag, little);
        put(
            &mut bytes,
            256 + index * word * 2 + word,
            word,
            value,
            little,
        );
    }
    for (index, (kind, offset, address, size)) in [
        (1, 0, 0x400000, 512),
        (2, 256, 0x400100, (entries.len() * word * 2) as u64),
        (3, 448, 0x4001c0, interpreter.len() as u64),
    ]
    .into_iter()
    .enumerate()
    {
        let start = header as usize + index * phsize as usize;
        put(&mut bytes, start, 4, kind, little);
        put(
            &mut bytes,
            start + if wide { 8 } else { 4 },
            word,
            offset,
            little,
        );
        put(
            &mut bytes,
            start + if wide { 16 } else { 8 },
            word,
            address,
            little,
        );
        put(
            &mut bytes,
            start + if wide { 32 } else { 16 },
            word,
            size,
            little,
        );
    }
    bytes
}

#[test]
fn elf_dependencies_work_for_both_classes_and_byte_orders_without_sections() {
    let scratch = Scratch::new();
    for wide in [false, true] {
        for little in [false, true] {
            let info = get_file_info(scratch.write("program", elf_fixture(wide, little))).unwrap();
            assert!(info.warnings.is_empty(), "{info}");
            assert_eq!(
                field(&info, "Format"),
                if wide { "ELF 64-bit" } else { "ELF 32-bit" }
            );
            assert_eq!(field(&info, "Linked libraries"), "libalpha.so\nlibbeta.so");
            assert_eq!(field(&info, "Loader"), "/lib/test-loader");
            assert_eq!(field(&info, "RUNPATH"), "/opt/lib");
            assert_eq!(field(&info, "Shared object name"), "sample.so");
        }
    }
}

#[test]
fn elf_without_dynamic_table_has_no_declared_libraries() {
    let scratch = Scratch::new();
    let mut bytes = elf_fixture(true, true);
    put(&mut bytes, 56, 2, 0, true); // e_phnum
    let info = get_file_info(scratch.write("static", bytes)).unwrap();
    assert!(info.warnings.is_empty(), "{info}");
    assert_eq!(field(&info, "Dynamic section"), "false");
    assert_eq!(
        field(&info, "Linked libraries"),
        "None declared (DT_NEEDED)"
    );
}

#[test]
fn malformed_elf_is_reported_without_panicking() {
    let scratch = Scratch::new();
    let good = elf_fixture(true, true);
    for length in [4, 15, 16, 51, 63, 127, 255, 511] {
        let info = get_file_info(scratch.write("truncated", &good[..length])).unwrap();
        assert_eq!(info.warnings.len(), 1, "length {length}: {info}");
        assert!(info.details.is_empty());
    }
    for (offset, width, value) in [
        (4, 1, 9),                       // unsupported class
        (5, 1, 0),                       // unsupported endian
        (32, 8, u64::MAX),               // overflowing program-header table
        (54, 2, 1),                      // undersized program-header entry
        (56, 2, 0xffff),                 // extended program-header count
        (256 + 2 * 16 + 8, 8, u64::MAX), // invalid DT_NEEDED string offset
        (256 + 8, 8, u64::MAX),          // unmapped DT_STRTAB address
        (256 + 6 * 16, 8, 1),            // missing DT_NULL
    ] {
        let mut bytes = good.clone();
        put(&mut bytes, offset, width, value, true);
        let info = get_file_info(scratch.write("malformed", bytes)).unwrap();
        assert_eq!(info.warnings.len(), 1, "offset {offset}: {info}");
    }
}

#[test]
fn cli_reports_errors_and_continues_to_later_paths() {
    let scratch = Scratch::new();
    let good = scratch.write("good.txt", "hello\n");
    let bad = scratch.write("bad.elf", b"\x7fELF");
    for path in [scratch.0.join("missing"), bad] {
        let output = Command::new(env!("CARGO_BIN_EXE_finfo"))
            .arg(path)
            .arg(&good)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&output.stdout).contains("good.txt"));
        assert!(String::from_utf8_lossy(&output.stderr).contains("1 file(s)"));
    }
    let output = Command::new(env!("CARGO_BIN_EXE_finfo"))
        .arg(good)
        .output()
        .unwrap();
    assert!(output.status.success());
}

#[test]
fn cli_help_includes_usage_and_github_links() {
    for args in [vec![], vec!["-h"], vec!["--help"]] {
        let output = Command::new(env!("CARGO_BIN_EXE_finfo"))
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        let help = String::from_utf8(output.stdout).unwrap();
        assert!(help.contains("Usage: finfo [OPTIONS] <PATH>..."));
        assert!(help.contains("Author:     https://github.com/aditya-gupta-dev\n"));
        assert!(help.contains("Repository: https://github.com/aditya-gupta-dev/finfo"));
    }
}

#[test]
fn cli_rejects_removed_and_unknown_flags() {
    for flag in ["--files", "--pids", "--unknown"] {
        let output = Command::new(env!("CARGO_BIN_EXE_finfo"))
            .arg(flag)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(error.contains(&format!("Unknown option: {flag}")));
        assert!(error.contains("finfo --help"));
    }
}

#[test]
fn cli_accepts_dash_prefixed_paths_after_separator() {
    let scratch = Scratch::new();
    scratch.write("--help", "literal file\n");
    let output = Command::new(env!("CARGO_BIN_EXE_finfo"))
        .current_dir(&scratch.0)
        .args(["--", "--help"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let report = String::from_utf8(output.stdout).unwrap();
    assert!(report.contains("--help"));
    assert!(report.contains("  Text\n"));
    assert!(!report.contains("Usage:"));

    let empty = Command::new(env!("CARGO_BIN_EXE_finfo"))
        .arg("--")
        .output()
        .unwrap();
    assert_eq!(empty.status.code(), Some(1));
}

#[test]
fn missing_path_returns_an_error() {
    let scratch = Scratch::new();
    assert_eq!(
        get_file_info(scratch.0.join("missing"))
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::NotFound
    );
}

#[test]
fn control_characters_in_filenames_are_escaped() {
    let scratch = Scratch::new();
    let path = scratch.write("escape-\x1b[31m", "text");
    let info = get_file_info(Path::new(&path)).unwrap();
    assert!(!info.to_string().contains('\x1b'));
}
