//! Загрузка списка источников из `urls.json`.
//!
//! Формат файла — объект `{ "1": "<url>", ... }`.

use std::collections::BTreeMap;
use std::path::Path;

use crate::error::{io_err, Error, Result};

/// Один источник подписки: `id` — имя выходного файла (`output/{id}.txt`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub id: String,
    pub url: String,
}

// Промежуточный тип строго под формат urls.json: объект {"id": "url"}.
// Парсим напрямую в BTreeMap (см. load_sources).

/// Загрузить и отсортировать источники по `id`.
///
/// Сортировка числовая там, где `id` — число (`"2" < "10"`),
/// иначе лексикографическая. Пустой файл — [`Error::EmptySources`].
pub fn load_sources(path: &Path) -> Result<Vec<Source>> {
    let raw = std::fs::read_to_string(path).map_err(|e| io_err(path, e))?;
    let parsed: BTreeMap<String, String> =
        serde_json::from_str(&raw).map_err(|e| Error::Parse {
            path: path.display().to_string(),
            source: e,
        })?;

    if parsed.is_empty() {
        return Err(Error::EmptySources {
            path: path.display().to_string(),
        });
    }

    let mut sources: Vec<Source> = parsed
        .into_iter()
        .filter(|(id, url)| !id.trim().is_empty() && !url.trim().is_empty())
        .map(|(id, url)| Source { id, url })
        .collect();

    // Числовая сортировка id, чтобы 2 шло раньше 10.
    sources.sort_by(|a, b| compare_ids(&a.id, &b.id));
    Ok(sources)
}

fn compare_ids(a: &str, b: &str) -> std::cmp::Ordering {
    match (a.parse::<u64>(), b.parse::<u64>()) {
        (Ok(x), Ok(y)) => x.cmp(&y),
        _ => a.cmp(b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_tmp(dir: &tempfile::TempDir, name: &str, content: &str) -> std::path::PathBuf {
        let path = dir.path().join(name);
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        path
    }

    #[test]
    fn loads_and_sorts_numerically() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_tmp(&dir, "urls.json", r#"{"10":"u10","2":"u2","1":"u1"}"#);
        let sources = load_sources(&path).unwrap();
        let ids: Vec<_> = sources.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, vec!["1", "2", "10"]);
    }

    #[test]
    fn rejects_empty_object() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_tmp(&dir, "urls.json", r#"{}"#);
        assert!(matches!(
            load_sources(&path),
            Err(Error::EmptySources { .. })
        ));
    }

    #[test]
    fn rejects_invalid_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_tmp(&dir, "urls.json", "not json");
        assert!(matches!(load_sources(&path), Err(Error::Parse { .. })));
    }

    #[test]
    fn reports_missing_file() {
        let err = load_sources(Path::new("/definitely/missing/urls.json")).unwrap_err();
        assert!(matches!(err, Error::Io { .. }));
    }
}
