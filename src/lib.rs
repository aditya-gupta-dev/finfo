
pub struct FileInfo {
    absolute_path: String,
    entity_type: Type,
    readonly: bool, 
    size: u64,
}

pub fn run(files: &Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let args_len = files.len(); 

    let files: Vec<&String> = files
        .iter()
        .filter(|file| file.file_exists().expect("failed to check file existence"))
        .collect();

    files.iter().for_each(|file| {
        if let Ok(info) = get_file_info(file) {
            println!("arg: {}", file);
            
            match info.entity_type {
                Type::File => println!("\ttype: file"),
                Type::Directory => println!("\ttype: directory"),
                Type::Symlink => println!("\ttype: symlink"),
            }

            println!("\tabsolute_path: {}", info.absolute_path);
            println!("\treadonly: {}", info.readonly);
            println!("\tsize: {}\n", format_size(info.size));
        }
    });

    let not_found_files = args_len - files.len(); 
    if not_found_files > 0 {
        println!("{} files were not found.", not_found_files);
    }

    Ok(())
}

pub fn get_file_info(file: &String) -> Result<FileInfo, Box<dyn std::error::Error>> {
    let metadata: std::fs::Metadata;

    match std::fs::metadata(file) {
        Ok(data) => {
            metadata = data;
        }
        Err(e) => {
            return Err(Box::new(e));
        }
    }

    let file_info = FileInfo {
        size: metadata.len(),
        entity_type: get_file_type(&metadata),
        readonly: metadata.permissions().readonly(), 
        absolute_path: std::fs::canonicalize(file)
            .unwrap()
            .to_string_lossy()
            .into_owned(),
    };

    Ok(file_info)
}

trait Exists {
    fn file_exists(&self) -> Result<bool, Box<dyn std::error::Error>>;
}

impl Exists for String {
    fn file_exists(&self) -> Result<bool, Box<dyn std::error::Error>> {
        if self.is_empty() {
            return Ok(false);
        }

        let mut is_exists = false;

        match std::fs::exists(self) {
            Ok(true) => is_exists = true,
            Ok(_) => {}
            Err(err) => {
                return Err(Box::new(err));
            }
        }

        return Ok(is_exists);
    }
}

fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];

    let mut size = bytes as f64;
    let mut unit = 0;

    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{} {}", bytes, UNITS[unit])
    } else {
        format!("{:.2} {}", size, UNITS[unit])
    }
}

enum Type {
    File,
    Directory,
    Symlink,
}

fn get_file_type(metadata: &std::fs::Metadata) -> Type {
    if metadata.is_dir() {
        Type::Directory
    } else if metadata.is_file() {
        Type::File
    } else {
        Type::Symlink
    }
}

