//! Разделение больших файлов конфигураций на части примерно по 1 МиБ.
//!
//! Каждая строка файла — одна конфигурация (`vless://…`, `trojan://…`, …),
//! поэтому резать можно только по границам строк: накапливаем целые строки,
//! пока часть не достигнет [`MAX_PART_BYTES`]. Последний кусок —
//! сколько останется. Разделение обратимо: конкатенация частей даёт исходник.

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// Максимальный размер одной части в байтах (1 МиБ).
pub const MAX_PART_BYTES: usize = 1024 * 1024;

/// Имя файла части: `output/{id}-{index}.txt` (нумерация с 1).
pub fn part_file_name(id: &str, index: usize) -> String {
    format!("{id}-{index}.txt")
}

/// Разделить контент на части не больше `max_bytes`, не разрывая строки.
///
/// - Пустой контент возвращает одну пустую часть.
/// - Строка длиннее `max_bytes` ложится в отдельную часть целиком
///   (часть при этом превышает лимит — строку рвать нельзя).
/// - Конкатенация частей всегда равна исходному контенту.
pub fn split_content(content: &str, max_bytes: usize) -> Vec<String> {
    let max_bytes = max_bytes.max(1);
    let mut parts: Vec<String> = Vec::new();
    let mut current = String::new();

    // `split_inclusive` сохраняет оригинальные окончания строк (`\n`, `\r\n`)
    // и последнюю строку без `\n`, поэтому сборка точная.
    for line in content.split_inclusive('\n') {
        if !current.is_empty() && current.len() + line.len() > max_bytes {
            parts.push(std::mem::take(&mut current));
        }
        current.push_str(line);
    }
    parts.push(current);

    parts
}

/// Отобрать кандидаты на разделение: `*.txt` без `-` в имени (то есть
/// исходные `output/{id}.txt`, а не уже нарезанные `{id}-{n}.txt`),
/// размером больше `max_bytes`. Возвращает пути, отсортированные по имени.
pub fn find_large_files(dir: &Path, max_bytes: u64) -> Result<Vec<PathBuf>> {
    let mut large = Vec::new();
    let mut entries = std::fs::read_dir(dir).map_err(|e| Error::Io {
        path: dir.display().to_string(),
        source: e,
    })?;
    while let Some(entry) = entries.next().transpose().map_err(|e| Error::Io {
        path: dir.display().to_string(),
        source: e,
    })? {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "txt") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        // Уже нарезанные части `{id}-{n}.txt` повторно не трогаем.
        if stem.contains('-') {
            continue;
        }
        let size = entry
            .metadata()
            .map_err(|e| Error::Io {
                path: path.display().to_string(),
                source: e,
            })?
            .len();
        if size > max_bytes {
            large.push(path);
        }
    }
    large.sort();
    Ok(large)
}

/// Разделить один существующий файл `output/{id}.txt` на `{id}-{n}.txt`.
///
/// Читает файл построчно (целиком в память как строку), пишет части через
/// [`split_content`] и удаляет исходник. Возвращает пути записанных частей.
/// Если файл меньше лимита — ничего не делает, возвращает пустой вектор.
pub fn split_file_if_large(path: &Path, max_bytes: usize) -> Result<Vec<PathBuf>> {
    let size = std::fs::metadata(path)
        .map_err(|e| Error::Io {
            path: path.display().to_string(),
            source: e,
        })?
        .len();
    if size <= max_bytes as u64 {
        return Ok(Vec::new());
    }

    let content = std::fs::read_to_string(path).map_err(|e| Error::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");

    let mut written = Vec::new();
    for (index, chunk) in split_content(&content, max_bytes).iter().enumerate() {
        let part = dir.join(part_file_name(id, index + 1));
        std::fs::write(&part, chunk).map_err(|e| Error::Save {
            path: part.display().to_string(),
            source: e,
        })?;
        written.push(part);
    }

    std::fs::remove_file(path).map_err(|e| Error::Save {
        path: path.display().to_string(),
        source: e,
    })?;
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn numbered_lines(count: usize) -> String {
        (0..count)
            .map(|i| format!("vless://user{i}@host:443#node-{i}\n"))
            .collect()
    }

    #[test]
    fn small_content_stays_single_part() {
        let content = numbered_lines(10);
        let parts = split_content(&content, MAX_PART_BYTES);
        assert_eq!(parts, vec![content]);
    }

    #[test]
    fn empty_content_is_single_empty_part() {
        assert_eq!(split_content("", 100), vec![String::new()]);
    }

    #[test]
    fn splits_by_lines_and_reassembles_exactly() {
        let content = numbered_lines(1000);
        let max = 1024;
        let parts = split_content(&content, max);
        assert!(parts.len() > 1);
        for part in &parts {
            assert!(part.len() <= max, "part of {} bytes", part.len());
            assert!(part.ends_with('\n'));
        }
        assert_eq!(parts.concat(), content);
    }

    #[test]
    fn last_part_holds_remainder() {
        // 10 строк по ~30 байт: при лимите 100 байт → 3+3+3+1.
        let content = numbered_lines(10);
        let line_len = "vless://user0@host:443#node-0\n".len();
        let parts = split_content(&content, line_len * 3);
        assert_eq!(parts.len(), 4);
        assert_eq!(parts[3].lines().count(), 1);
        assert_eq!(parts.concat(), content);
    }

    #[test]
    fn oversized_single_line_is_not_torn() {
        let long = format!("vless://{}@host:443#big\n", "x".repeat(500));
        let content = format!("{long}vless://a@b:443#s\n");
        let parts = split_content(&content, 100);
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0], long);
        assert_eq!(parts.concat(), content);
    }

    #[test]
    fn preserves_missing_trailing_newline() {
        let content = "vless://a@b:443#one\ntrojan://p@h:443#two".to_string();
        let parts = split_content(&content, 24);
        assert_eq!(parts.concat(), content);
        assert!(!parts.last().unwrap().ends_with('\n'));
    }

    #[test]
    fn exact_limit_fits_in_one_part() {
        let content = "vless://a@b:1#x\n".to_string();
        let parts = split_content(&content, content.len());
        assert_eq!(parts, vec![content]);
    }

    #[test]
    fn part_names_are_one_based() {
        assert_eq!(part_file_name("1", 1), "1-1.txt");
        assert_eq!(part_file_name("21", 12), "21-12.txt");
    }

    #[test]
    fn small_file_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("7.txt");
        std::fs::write(&path, "vless://a@b:443#x\n").unwrap();
        assert!(split_file_if_large(&path, MAX_PART_BYTES)
            .unwrap()
            .is_empty());
        assert!(path.exists());
    }

    #[test]
    fn large_file_is_split_and_source_removed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("9.txt");
        let content = numbered_lines(200);
        std::fs::write(&path, &content).unwrap();

        let line_len = "vless://user0@host:443#node-0\n".len();
        let written = split_file_if_large(&path, line_len * 10).unwrap();

        assert!(!path.exists(), "исходник должен быть удалён");
        assert!(written.len() > 1);
        let reassembled: String = written
            .iter()
            .map(std::fs::read_to_string)
            .collect::<std::io::Result<String>>()
            .unwrap();
        assert_eq!(reassembled, content);
        assert_eq!(written[0], dir.path().join("9-1.txt"));
    }

    #[test]
    fn find_large_files_skips_parts_and_small() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("1.txt"), vec![b'x'; 200]).unwrap();
        std::fs::write(dir.path().join("2.txt"), "tiny").unwrap();
        std::fs::write(dir.path().join("1-1.txt"), vec![b'y'; 500]).unwrap();

        let found = find_large_files(dir.path(), 100).unwrap();
        assert_eq!(found, vec![dir.path().join("1.txt")]);
    }
}
