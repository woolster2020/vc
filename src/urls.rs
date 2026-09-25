//! Индекс ссылок `output/urls.txt`.
//!
//! Для каждого `*.txt` в каталоге вывода (кроме самого `urls.txt`)
//! пишется одна строка вида:
//! `https://raw.githubusercontent.com/woolster2020/vc/refs/heads/main/output/1-3.txt`
//! (прямые ссылки без CDN-кеширования).
//!
//! Сортировка — естественная (numeric-aware): `2-2.txt` идёт раньше
//! `2-10.txt`, а `2-*.txt` — раньше `10-*.txt` (в отличие от чистой
//! лексикографической сортировки `sort`, где `"10-1" < "2-1"`).

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// База прямых ссылок (`raw.githubusercontent.com`) на файлы из `output/`.
pub const RAW_BASE: &str =
    "https://raw.githubusercontent.com/woolster2020/vc/refs/heads/main/output";

/// Имя файла-индекса внутри каталога вывода.
pub const URLS_FILE_NAME: &str = "urls.txt";

/// Прямая ссылка для имени файла (без каталога), например `1-3.txt`.
pub fn raw_url_for(file_name: &str) -> String {
    format!("{RAW_BASE}/{file_name}")
}

/// Собрать имена `*.txt`-файлов в `dir`, кроме самого `urls.txt`.
///
/// Возвращает имена файлов (не пути), отсортированные естественным порядком.
/// Не-`.txt` файлы и подкаталоги игнорируются. Отсутствующий каталог —
/// пустой список (индекс будет пустым, а не ошибкой).
pub fn collect_output_files(dir: &Path) -> Result<Vec<String>> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => {
            return Err(Error::Io {
                path: dir.display().to_string(),
                source: e,
            });
        }
    };

    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| Error::Io {
            path: dir.display().to_string(),
            source: e,
        })?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path.extension().is_none_or(|ext| ext != "txt") {
            continue;
        }
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if name == URLS_FILE_NAME {
            continue;
        }
        names.push(name.to_string());
    }
    names.sort_by(|a, b| compare_natural(a, b));
    Ok(names)
}

/// Записать `output/urls.txt`: по одной прямой ссылке на строку.
///
/// Возвращает путь записанного индекса. Каталог создаётся при необходимости.
/// Каждая строка заканчивается `\n` (в конце файла тоже `\n`, если есть строки).
pub fn write_urls_file(dir: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(dir).map_err(|e| Error::Save {
        path: dir.display().to_string(),
        source: e,
    })?;

    let names = collect_output_files(dir)?;
    let mut content = String::new();
    for name in &names {
        content.push_str(&raw_url_for(name));
        content.push('\n');
    }

    let path = dir.join(URLS_FILE_NAME);
    std::fs::write(&path, content).map_err(|e| Error::Save {
        path: path.display().to_string(),
        source: e,
    })?;
    Ok(path)
}

/// Естественное сравнение: последовательности цифр сравниваются как числа,
/// остальные фрагменты — лексикографически.
fn compare_natural(a: &str, b: &str) -> std::cmp::Ordering {
    let mut a_chunks = split_chunks(a);
    let mut b_chunks = split_chunks(b);
    // Сравнение по чанкам; недостающий чанк = меньше (короткое раньше).
    loop {
        match (a_chunks.pop(), b_chunks.pop()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, Some(_)) => return std::cmp::Ordering::Less,
            (Some(_), None) => return std::cmp::Ordering::Greater,
            (Some(x), Some(y)) => {
                let ord = compare_chunks(&x, &y);
                if ord != std::cmp::Ordering::Equal {
                    return ord;
                }
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Chunk {
    Num(u64, String),
    Str(String),
}

/// Разбить строку на чанки «цифры / не-цифры» (в прямом порядке, для `pop`
/// с конца храним перевёрнутый вектор — так проще без индексов).
fn split_chunks(s: &str) -> Vec<Chunk> {
    let mut chunks: Vec<Chunk> = Vec::new();
    let mut buf = String::new();
    let mut buf_is_num: Option<bool> = None;

    let flush = |buf: &mut String, is_num: bool, out: &mut Vec<Chunk>| {
        if buf.is_empty() {
            return;
        }
        let raw = std::mem::take(buf);
        if is_num {
            // Длинные числа, не влезающие в u64, сравниваем по длине,
            // затем лексикографически (без ведущих нулей — по строке).
            let trimmed = raw.trim_start_matches('0');
            let num = trimmed.parse::<u64>().unwrap_or(u64::MAX);
            out.push(Chunk::Num(num, raw));
        } else {
            out.push(Chunk::Str(raw));
        }
    };

    for ch in s.chars() {
        let is_num = ch.is_ascii_digit();
        match buf_is_num {
            Some(prev) if prev == is_num => buf.push(ch),
            Some(prev) => {
                flush(&mut buf, prev, &mut chunks);
                buf.push(ch);
                buf_is_num = Some(is_num);
            }
            None => {
                buf.push(ch);
                buf_is_num = Some(is_num);
            }
        }
    }
    if let Some(is_num) = buf_is_num {
        flush(&mut buf, is_num, &mut chunks);
    }
    chunks.reverse();
    chunks
}

fn compare_chunks(a: &Chunk, b: &Chunk) -> std::cmp::Ordering {
    match (a, b) {
        (Chunk::Num(an, ar), Chunk::Num(bn, br)) => {
            // Сначала числовое значение, затем — сырая строка
            // (стабильность при ведущих нулях: "02" vs "2").
            an.cmp(bn).then_with(|| ar.cmp(br))
        }
        (Chunk::Num(_, _), Chunk::Str(_)) => std::cmp::Ordering::Less,
        (Chunk::Str(_), Chunk::Num(_, _)) => std::cmp::Ordering::Greater,
        (Chunk::Str(x), Chunk::Str(y)) => x.cmp(y),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_url_joins_base_and_name() {
        assert_eq!(
            raw_url_for("1-3.txt"),
            "https://raw.githubusercontent.com/woolster2020/vc/refs/heads/main/output/1-3.txt"
        );
    }

    #[test]
    fn natural_order_puts_2_before_10_and_2_before_2_10() {
        let mut names = vec!["10-1.txt", "2-10.txt", "2-2.txt", "1-3.txt", "11.txt"];
        names.sort_by(|a, b| compare_natural(a, b));
        assert_eq!(
            names,
            vec!["1-3.txt", "2-2.txt", "2-10.txt", "10-1.txt", "11.txt"]
        );
    }

    #[test]
    fn collect_skips_urls_txt_non_txt_and_dirs() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("2-10.txt"), "b").unwrap();
        std::fs::write(dir.path().join("2-2.txt"), "a").unwrap();
        std::fs::write(dir.path().join("urls.txt"), "stale").unwrap();
        std::fs::write(dir.path().join("notes.md"), "doc").unwrap();
        std::fs::create_dir(dir.path().join("1-1.txt")).unwrap();

        let names = collect_output_files(dir.path()).unwrap();
        assert_eq!(names, vec!["2-2.txt", "2-10.txt"]);
    }

    #[test]
    fn collect_missing_dir_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let names = collect_output_files(&dir.path().join("nope")).unwrap();
        assert!(names.is_empty());
    }

    #[test]
    fn write_produces_one_url_per_line_with_trailing_newline() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("1-3.txt"), "x").unwrap();
        std::fs::write(dir.path().join("2.txt"), "y").unwrap();

        let path = write_urls_file(dir.path()).unwrap();
        assert_eq!(path, dir.path().join("urls.txt"));

        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            content,
            "https://raw.githubusercontent.com/woolster2020/vc/refs/heads/main/output/1-3.txt\n\
             https://raw.githubusercontent.com/woolster2020/vc/refs/heads/main/output/2.txt\n"
        );
    }

    #[test]
    fn write_empty_dir_produces_empty_index() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_urls_file(dir.path()).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "");
    }

    #[test]
    fn write_overwrites_stale_index_and_excludes_itself() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("urls.txt"), "stale\n").unwrap();
        std::fs::write(dir.path().join("5.txt"), "x").unwrap();

        write_urls_file(dir.path()).unwrap();
        let content = std::fs::read_to_string(dir.path().join("urls.txt")).unwrap();
        assert_eq!(
            content,
            "https://raw.githubusercontent.com/woolster2020/vc/refs/heads/main/output/5.txt\n"
        );
        assert!(!content.contains("urls.txt\nurls"));
    }
}
