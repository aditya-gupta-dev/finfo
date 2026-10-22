//! Minimal, read-only ELF metadata inspection (32/64-bit, either byte order).
//! Layouts: https://gabi.xinuos.com/elf/02-eheader.html and 07/08 chapters.
//! No commands are invoked and no code from the inspected file is loaded.

use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom};

use super::{Details, FileAnalyzer};
use crate::files::FileContext;

const PT_LOAD: u64 = 1;
const PT_DYNAMIC: u64 = 2;
const PT_INTERP: u64 = 3;
const DT_NULL: u64 = 0;
const DT_NEEDED: u64 = 1;
const DT_STRTAB: u64 = 5;
const DT_STRSZ: u64 = 10;
const DT_SONAME: u64 = 14;
const DT_RPATH: u64 = 15;
const DT_RUNPATH: u64 = 29;

pub struct ElfAnalyzer;

impl FileAnalyzer for ElfAnalyzer {
    fn name(&self) -> &'static str {
        "ELF"
    }

    fn supports(&self, file: &FileContext) -> bool {
        file.metadata.is_file() && file.sample.starts_with(b"\x7fELF")
    }

    fn analyze(&self, file: &FileContext) -> io::Result<Details> {
        ElfReader::open(file)?.inspect()
    }
}

struct Segment {
    kind: u64,
    offset: u64,
    address: u64,
    size: u64,
}

struct ElfReader {
    input: BufReader<File>,
    length: u64,
    word_size: usize,
    little_endian: bool,
}

impl ElfReader {
    fn open(file: &FileContext) -> io::Result<Self> {
        if file.sample.len() < 16 || file.sample[6] != 1 {
            return Err(invalid("missing or unsupported ELF identification"));
        }
        let word_size = match file.sample[4] {
            1 => 4,
            2 => 8,
            _ => return Err(invalid("unsupported ELF class")),
        };
        let little_endian = match file.sample[5] {
            1 => true,
            2 => false,
            _ => return Err(invalid("unsupported ELF byte order")),
        };
        let input = File::open(&file.path)?;
        Ok(Self {
            length: input.metadata()?.len(),
            input: BufReader::new(input),
            word_size,
            little_endian,
        })
    }

    fn inspect(&mut self) -> io::Result<Details> {
        let wide = self.word_size == 8;
        let header_size = if wide { 64 } else { 52 };
        let header = self.read_at(0, header_size)?;
        if self.number(&header, 20, 4) != 1 {
            return Err(invalid("unsupported ELF version"));
        }
        let entry = self.number(&header, 24, self.word_size);
        let phoff = self.number(&header, if wide { 32 } else { 28 }, self.word_size);
        let sizes = if wide { 52 } else { 40 };
        if self.number(&header, sizes, 2) < header_size as u64 {
            return Err(invalid("ELF header is too small"));
        }
        let phentsize = self.number(&header, sizes + 2, 2);
        let phnum = self.number(&header, sizes + 4, 2);
        if phnum == 0xffff {
            return Err(invalid(
                "extended ELF program-header counts are unsupported",
            ));
        }
        let segment_size = if wide { 56 } else { 32 };
        if phnum > 0 && phentsize < segment_size as u64 {
            return Err(invalid("ELF program-header entry is too small"));
        }
        self.check_range(phoff, phnum * phentsize)?;
        let mut segments = Vec::new();
        for index in 0..phnum {
            let bytes = self.read_at(phoff + index * phentsize, segment_size)?;
            let segment = Segment {
                kind: self.number(&bytes, 0, 4),
                offset: self.number(&bytes, if wide { 8 } else { 4 }, self.word_size),
                address: self.number(&bytes, if wide { 16 } else { 8 }, self.word_size),
                size: self.number(&bytes, if wide { 32 } else { 16 }, self.word_size),
            };
            self.check_range(segment.offset, segment.size)?;
            segments.push(segment);
        }

        let mut details = Details::new("ELF");
        details.add("Format", format!("ELF {}-bit", self.word_size * 8));
        details.add(
            "Byte order",
            if self.little_endian {
                "Little endian"
            } else {
                "Big endian"
            },
        );
        let machine = self.number(&header, 18, 2);
        let architecture = match machine {
            3 => "x86",
            8 => "MIPS",
            20 => "PowerPC",
            21 => "PowerPC64",
            40 => "ARM",
            62 => "x86-64",
            183 => "AArch64",
            243 => "RISC-V",
            _ => "Other",
        };
        details.add(
            "Architecture",
            format!("{architecture} (machine {machine})"),
        );
        let kind = match self.number(&header, 16, 2) {
            1 => "Relocatable object",
            2 => "Executable",
            3 => "Shared object / position-independent executable",
            4 => "Core dump",
            _ => "Other",
        };
        details.add("ELF type", kind);
        details.add("Entry point", format!("0x{entry:x}"));
        details.add("Program headers", phnum);
        if let Some(segment) = segments.iter().find(|s| s.kind == PT_INTERP) {
            details.add("Loader", self.read_string(segment.offset, segment.size)?);
        }
        self.dynamic_details(&segments, &mut details)?;
        Ok(details)
    }

    fn dynamic_details(&mut self, segments: &[Segment], details: &mut Details) -> io::Result<()> {
        let mut strings = Vec::new();
        let (mut strtab, mut strsize) = (None, None);
        let dynamic = segments.iter().find(|s| s.kind == PT_DYNAMIC);
        details.add("Dynamic section", dynamic.is_some());
        if let Some(segment) = dynamic {
            let entry_size = (self.word_size * 2) as u64;
            let mut terminated = false;
            for index in 0..segment.size / entry_size {
                let bytes =
                    self.read_at(segment.offset + index * entry_size, entry_size as usize)?;
                let tag = self.number(&bytes, 0, self.word_size);
                let value = self.number(&bytes, self.word_size, self.word_size);
                match tag {
                    DT_NULL => {
                        terminated = true;
                        break;
                    }
                    DT_STRTAB => strtab = Some(value),
                    DT_STRSZ => strsize = Some(value),
                    DT_NEEDED | DT_SONAME | DT_RPATH | DT_RUNPATH => strings.push((tag, value)),
                    _ => {}
                }
            }
            if !terminated {
                return Err(invalid("unterminated ELF dynamic table"));
            }
        }
        let mut libraries = Vec::new();
        if !strings.is_empty() {
            let address = strtab.ok_or_else(|| invalid("missing ELF dynamic string table"))?;
            let size = strsize.ok_or_else(|| invalid("missing ELF dynamic string table size"))?;
            // DT_STRTAB is a virtual address; translate through a file-backed PT_LOAD.
            let offset = segments
                .iter()
                .filter(|s| s.kind == PT_LOAD)
                .find_map(|segment| {
                    let delta = address.checked_sub(segment.address)?;
                    if delta <= segment.size && size <= segment.size - delta {
                        segment.offset.checked_add(delta)
                    } else {
                        None
                    }
                })
                .ok_or_else(|| invalid("ELF string table is outside loadable file data"))?;
            for (tag, index) in strings {
                if index >= size {
                    return Err(invalid("ELF string index is out of bounds"));
                }
                let value = self.read_string(offset + index, size - index)?;
                match tag {
                    DT_NEEDED => libraries.push(value),
                    DT_SONAME => details.add("Shared object name", value),
                    DT_RPATH => details.add("RPATH", value),
                    DT_RUNPATH => details.add("RUNPATH", value),
                    _ => unreachable!(),
                }
            }
        }
        details.add(
            "Linked libraries",
            if libraries.is_empty() {
                "None declared (DT_NEEDED)".to_owned()
            } else {
                libraries.join("\n")
            },
        );
        Ok(())
    }

    fn check_range(&self, offset: u64, size: u64) -> io::Result<()> {
        if offset > self.length || size > self.length - offset {
            Err(invalid("ELF offset or size is outside the file"))
        } else {
            Ok(())
        }
    }

    // Only fixed-size metadata structures are read here, never a whole executable.
    fn read_at(&mut self, offset: u64, size: usize) -> io::Result<Vec<u8>> {
        self.check_range(offset, size as u64)?;
        self.input.seek(SeekFrom::Start(offset))?;
        let mut bytes = vec![0; size];
        self.input.read_exact(&mut bytes)?;
        Ok(bytes)
    }

    fn read_string(&mut self, offset: u64, available: u64) -> io::Result<String> {
        self.check_range(offset, available)?;
        self.input.seek(SeekFrom::Start(offset))?;
        let mut bytes = Vec::new();
        // Bound allocations even if a corrupt input has no terminating NUL.
        self.input
            .by_ref()
            .take(available.min(65536))
            .read_until(0, &mut bytes)?;
        if bytes.pop() != Some(0) {
            return Err(invalid("unterminated or oversized ELF string"));
        }
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }

    fn number(&self, bytes: &[u8], offset: usize, width: usize) -> u64 {
        // Offsets and widths are constants for previously validated structure sizes.
        let bytes = &bytes[offset..offset + width];
        if self.little_endian {
            bytes
                .iter()
                .rev()
                .fold(0, |value, byte| (value << 8) | u64::from(*byte))
        } else {
            bytes
                .iter()
                .fold(0, |value, byte| (value << 8) | u64::from(*byte))
        }
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
