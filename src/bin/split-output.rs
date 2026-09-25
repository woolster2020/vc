//! Разовая миграция: нарезать существующие `output/{id}.txt` больше лимита
//! на построчные части `output/{id}-{n}.txt`, исходники удалить.
//!
//! Использование:
//! ```sh
//! cargo run --bin split-output -- [output_dir] [max_per_file]
//! ```
//! По умолчанию `output` и 500 конфигураций на файл.
//!
//! Трогает только `*.txt` без `-` в имени, где конфигураций больше лимита;
//! уже нарезанные `{id}-{n}.txt` и маленькие файлы не изменяются.

use std::path::PathBuf;
use std::process::ExitCode;

use vc::{find_large_files, split_file_if_large, write_urls_file, DEFAULT_MAX_PER_FILE};

fn main() -> ExitCode {
    let mut cli = std::env::args().skip(1);
    let dir: PathBuf = cli
        .next()
        .map_or_else(|| PathBuf::from("output"), PathBuf::from);
    let max_per_file: usize = cli
        .next()
        .and_then(|v| v.trim().parse().ok())
        .filter(|n| *n > 0)
        .unwrap_or(DEFAULT_MAX_PER_FILE);

    let large = match find_large_files(&dir, max_per_file) {
        Ok(found) => found,
        Err(e) => {
            eprintln!("error: cannot scan {}: {e}", dir.display());
            return ExitCode::FAILURE;
        }
    };

    if large.is_empty() {
        println!("no files over {max_per_file} configs in {}", dir.display());
    } else {
        let mut total_parts = 0usize;
        for path in &large {
            match split_file_if_large(path, max_per_file) {
                Ok(parts) => {
                    total_parts += parts.len();
                    println!(
                        "split {} ({} part(s)): {} .. {}",
                        path.display(),
                        parts.len(),
                        parts
                            .first()
                            .map_or("-".into(), |p| p.display().to_string()),
                        parts.last().map_or("-".into(), |p| p.display().to_string()),
                    );
                }
                Err(e) => {
                    eprintln!("fail {}: {e}", path.display());
                    return ExitCode::FAILURE;
                }
            }
        }
        println!(
            "done: {} file(s) -> {total_parts} part(s), sources removed",
            large.len()
        );
    }

    match write_urls_file(&dir) {
        Ok(index) => {
            println!("ok   urls: {} (index)", index.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("fail urls.txt: {e}");
            ExitCode::FAILURE
        }
    }
}
