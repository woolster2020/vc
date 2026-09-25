//! Разовая миграция: нарезать существующие `output/{id}.txt` больше 1 МиБ
//! на построчные части `output/{id}-{n}.txt`, исходники удалить.
//!
//! Использование:
//! ```sh
//! cargo run --bin split-output -- [output_dir]
//! ```
//!
//! Трогает только `*.txt` без `-` в имени размером больше лимита;
//! уже нарезанные `{id}-{n}.txt` и маленькие файлы не изменяются.

use std::path::PathBuf;
use std::process::ExitCode;

use vc::{find_large_files, split_file_if_large, MAX_PART_BYTES};

fn main() -> ExitCode {
    let dir: PathBuf = std::env::args()
        .nth(1)
        .map_or_else(|| PathBuf::from("output"), PathBuf::from);

    let large = match find_large_files(&dir, MAX_PART_BYTES as u64) {
        Ok(found) => found,
        Err(e) => {
            eprintln!("error: cannot scan {}: {e}", dir.display());
            return ExitCode::FAILURE;
        }
    };

    if large.is_empty() {
        println!(
            "no files over {} bytes in {}",
            MAX_PART_BYTES,
            dir.display()
        );
        return ExitCode::SUCCESS;
    }

    let mut total_parts = 0usize;
    for path in &large {
        match split_file_if_large(path, MAX_PART_BYTES) {
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
    ExitCode::SUCCESS
}
