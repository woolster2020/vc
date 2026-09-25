//! Разделение больших файлов конфигураций на части по N конфигов.
//!
//! Каждая строка файла — одна конфигурация (`vless://…`, `trojan://…`, …),
//! поэтому часть — это первые `max_per_file` непустых строк, следующая —
//! следующие и т.д. Последний кусок — сколько останется (или меньше).
//! Разделение обратимо: конкатенация частей даёт исходник.

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// Сколько конфигураций кладём в одну часть по умолчанию.
pub const DEFAULT_MAX_PER_FILE: usize = 500;

/// Имя файла части: `output/{id}-{index}.txt` (нумерация с 1).
pub fn part_file_name(id: &str, index: usize) -> String {
    format!("{id}-{index}.txt")
}

/// Конфигурация — это непустая строка (пустые/пробельные строками
/// переносятся как есть, но в лимит не считаются).
pub fn is_config_line(line: &str) -> bool {
    !line.trim().is_empty()
}

/// Число конфигураций в контенте.
pub fn count_configs(content: &str) -> usize {
    content.lines().filter(|line| is_config_line(line)).count()
}

/// Разделить контент на части максимум по `max_per_file` конфигураций.
///
/// - Пустой контент возвращает одну пустую часть.
/// - Пустые строки лимит не тратят, но сохраняются на своих местах.
/// - Конкатенация частей всегда равна исходному контенту.
pub fn split_content(content: &str, max_per_file: usize) -> Vec<String> {
    let max_per_file = max_per_file.max(1);
    let mut parts: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut count = 0usize;

    // `split_inclusive` сохраняет оригинальные окончания строк (`\n`, `\r\n`)
    // и последнюю строку без `\n`, поэтому сборка точная.
    for line in content.split_inclusive('\n') {
        if is_config_line(line) {
            if count >= max_per_file {
                parts.push(std::mem::take(&mut current));
                count = 0;
            }
            count += 1;
        }
        current.push_str(line);
    }
    parts.push(current);

    parts
}

/// Отобрать кандидаты на разделение: `*.txt` без `-` в имени (то есть
/// исходные `output/{id}.txt`, а не уже нарезанные `{id}-{n}.txt`),
/// в которых конфигураций больше `max_per_file`.
/// Возвращает пути, отсортированные по имени.
pub fn find_large_files(dir: &Path, max_per_file: usize) -> Result<Vec<PathBuf>> {
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
        let content = std::fs::read_to_string(&path).map_err(|e| Error::Io {
            path: path.display().to_string(),
            source: e,
        })?;
        if count_configs(&content) > max_per_file {
            large.push(path);
        }
    }
    large.sort();
    Ok(large)
}

/// Разделить один существующий файл `output/{id}.txt` на `{id}-{n}.txt`.
///
/// Читает файл построчно, пишет части через [`split_content`]
/// и удаляет исходник. Возвращает пути записанных частей.
/// Если конфигураций не больше лимита — ничего не делает,
/// возвращает пустой вектор.
pub fn split_file_if_large(path: &Path, max_per_file: usize) -> Result<Vec<PathBuf>> {
    let content = std::fs::read_to_string(path).map_err(|e| Error::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    if count_configs(&content) <= max_per_file.max(1) {
        return Ok(Vec::new());
    }

    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");

    let mut written = Vec::new();
    for (index, chunk) in split_content(&content, max_per_file).iter().enumerate() {
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
        let parts = split_content(&content, DEFAULT_MAX_PER_FILE);
        assert_eq!(parts, vec![content]);
    }

    #[test]
    fn empty_content_is_single_empty_part() {
        assert_eq!(split_content("", 500), vec![String::new()]);
    }

    #[test]
    fn splits_every_n_configs_and_reassembles_exactly() {
        let content = numbered_lines(12);
        let parts = split_content(&content, 5);
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0].lines().count(), 5);
        assert_eq!(parts[1].lines().count(), 5);
        // Последний кусок — сколько останется.
        assert_eq!(parts[2].lines().count(), 2);
        for part in &parts {
            assert!(count_configs(part) <= 5);
        }
        assert_eq!(parts.concat(), content);
    }

    #[test]
    fn exact_limit_fits_in_one_part() {
        let content = numbered_lines(5);
        let parts = split_content(&content, 5);
        assert_eq!(parts, vec![content]);
    }

    #[test]
    fn blank_lines_do_not_consume_limit_but_survive() {
        let content = "vless://a@h:1#x\n\nvless://b@h:2#y\n   \n".to_string();
        let parts = split_content(&content, 1);
        assert_eq!(parts.len(), 2);
        assert_eq!(parts.concat(), content);
        assert_eq!(count_configs(&content), 2);
    }

    #[test]
    fn preserves_missing_trailing_newline() {
        let content = "vless://a@b:443#one\ntrojan://p@h:443#two".to_string();
        let parts = split_content(&content, 1);
        assert_eq!(parts.len(), 2);
        assert_eq!(parts.concat(), content);
        assert!(!parts.last().unwrap().ends_with('\n'));
    }

    #[test]
    fn zero_limit_means_one_config_per_part() {
        let parts = split_content(&numbered_lines(3), 0);
        assert_eq!(parts.len(), 3);
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
        assert!(split_file_if_large(&path, DEFAULT_MAX_PER_FILE)
            .unwrap()
            .is_empty());
        assert!(path.exists());
    }

    #[test]
    fn large_file_is_split_and_source_removed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("9.txt");
        let content = numbered_lines(12);
        std::fs::write(&path, &content).unwrap();

        let written = split_file_if_large(&path, 5).unwrap();

        assert!(!path.exists(), "исходник должен быть удалён");
        assert_eq!(written.len(), 3);
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
        std::fs::write(dir.path().join("1.txt"), numbered_lines(10)).unwrap();
        std::fs::write(dir.path().join("2.txt"), "vless://a@b:1#x\n").unwrap();
        std::fs::write(dir.path().join("1-1.txt"), numbered_lines(50)).unwrap();

        let found = find_large_files(dir.path(), 5).unwrap();
        assert_eq!(found, vec![dir.path().join("1.txt")]);
    }
}
