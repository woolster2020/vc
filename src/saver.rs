//! Сохранение декодированных конфигов в `output/{id}.txt`.
//!
//! Файлы больше `max_per_file` конфигураций построчно нарезаются на части
//! `output/{id}-{n}.txt` (последняя — сколько останется),
//! исходный `output/{id}.txt` при этом удаляется.
//!
//! `Saver` — порт (трейт), `FileSaver` — файловый адаптер.

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::splitter::{count_configs, part_file_name, split_content, DEFAULT_MAX_PER_FILE};

/// Порт сохранения. Возвращает пути записанных файлов
/// (один элемент для маленьких файлов, несколько для нарезанных).
pub trait Saver: Send + Sync {
    /// Сохранить `content` как `{output_dir}/{id}.txt`
    /// либо как `{output_dir}/{id}-{n}.txt`, если контент больше лимита.
    fn save(
        &self,
        id: &str,
        content: &str,
    ) -> impl std::future::Future<Output = Result<Vec<PathBuf>>> + Send;
}

/// Файловый сейвер. Директория создаётся лениво при первом `save`.
pub struct FileSaver {
    output_dir: PathBuf,
    max_per_file: usize,
}

impl FileSaver {
    pub fn new(output_dir: impl Into<PathBuf>) -> Self {
        Self {
            output_dir: output_dir.into(),
            max_per_file: DEFAULT_MAX_PER_FILE,
        }
    }

    /// Конструктор с кастомным лимитом конфигураций на файл (для тестов и CLI).
    pub fn with_max_per_file(output_dir: impl Into<PathBuf>, max_per_file: usize) -> Self {
        Self {
            output_dir: output_dir.into(),
            max_per_file: max_per_file.max(1),
        }
    }

    /// Путь файла для `id` без записи.
    pub fn path_for(&self, id: &str) -> PathBuf {
        self.output_dir.join(format!("{id}.txt"))
    }

    /// Путь n-й части (нумерация с 1) без записи.
    pub fn part_path_for(&self, id: &str, index: usize) -> PathBuf {
        self.output_dir.join(part_file_name(id, index))
    }

    /// Удалить устаревшие части `{id}-{n}.txt` начиная с `keep_from`
    /// (например, остаток от прошлого, более крупного сплита).
    /// Пропавшие файлы — не ошибка.
    async fn remove_stale_parts(&self, id: &str, keep_from: usize) {
        let mut index = keep_from;
        loop {
            let stale = self.part_path_for(id, index);
            match tokio::fs::remove_file(&stale).await {
                Ok(()) => index += 1,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => break,
                // Прерываемся на первой «дырке»: части пишутся подряд с 1.
                Err(_) => break,
            }
        }
    }
}

impl Saver for FileSaver {
    async fn save(&self, id: &str, content: &str) -> Result<Vec<PathBuf>> {
        tokio::fs::create_dir_all(&self.output_dir)
            .await
            .map_err(|e| parent_path_error(&self.output_dir, e))?;

        if count_configs(content) <= self.max_per_file {
            let path = self.path_for(id);
            tokio::fs::write(&path, content)
                .await
                .map_err(|e| Error::Save {
                    path: path.display().to_string(),
                    source: e,
                })?;
            // Контент ужался до одной части — вычищаем остатки прошлого сплита.
            self.remove_stale_parts(id, 1).await;
            return Ok(vec![path]);
        }

        let mut written = Vec::new();
        for (index, chunk) in split_content(content, self.max_per_file).iter().enumerate() {
            let part = self.part_path_for(id, index + 1);
            tokio::fs::write(&part, chunk)
                .await
                .map_err(|e| Error::Save {
                    path: part.display().to_string(),
                    source: e,
                })?;
            written.push(part);
        }
        // Вычищаем хвост от прошлого, более крупного сплита.
        self.remove_stale_parts(id, written.len() + 1).await;
        // Исходный большой файл не храним — только части.
        let single = self.path_for(id);
        match tokio::fs::remove_file(&single).await {
            Ok(()) | Err(_) => {}
        }
        Ok(written)
    }
}

fn parent_path_error(parent: &Path, source: std::io::Error) -> Error {
    Error::Save {
        path: parent.display().to_string(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn writes_id_txt_and_creates_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("nested").join("output");
        let saver = FileSaver::new(&out);

        let paths = saver.save("7", "vless://x\n").await.unwrap();
        assert_eq!(paths, vec![out.join("7.txt")]);
        assert_eq!(std::fs::read_to_string(&paths[0]).unwrap(), "vless://x\n");
    }

    #[tokio::test]
    async fn overwrites_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let saver = FileSaver::new(dir.path());
        saver.save("1", "first").await.unwrap();
        saver.save("1", "second").await.unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("1.txt")).unwrap(),
            "second"
        );
    }

    fn numbered_lines(count: usize) -> String {
        (0..count)
            .map(|i| format!("vless://user{i}@host:443#node-{i}\n"))
            .collect()
    }

    #[tokio::test]
    async fn splits_large_content_into_parts_and_removes_single() {
        let dir = tempfile::tempdir().unwrap();
        let saver = FileSaver::with_max_per_file(dir.path(), 7);
        // Лежит старый большой `1.txt` — после сплита его быть не должно.
        std::fs::write(dir.path().join("1.txt"), "stale").unwrap();

        let content = numbered_lines(20);
        let paths = saver.save("1", &content).await.unwrap();

        assert_eq!(paths.len(), 3);
        assert_eq!(paths[0], dir.path().join("1-1.txt"));
        assert!(!dir.path().join("1.txt").exists());
        assert_eq!(
            count_configs(&std::fs::read_to_string(&paths[0]).unwrap()),
            7
        );
        assert_eq!(
            count_configs(&std::fs::read_to_string(&paths[2]).unwrap()),
            6
        );
        let reassembled: String = paths
            .iter()
            .map(std::fs::read_to_string)
            .collect::<std::io::Result<String>>()
            .unwrap();
        assert_eq!(reassembled, content);
    }

    #[tokio::test]
    async fn shrinking_back_to_single_removes_stale_parts() {
        let dir = tempfile::tempdir().unwrap();
        let saver = FileSaver::with_max_per_file(dir.path(), 7);

        let big = numbered_lines(20);
        let parts = saver.save("2", &big).await.unwrap();
        assert_eq!(parts.len(), 3);

        let paths = saver.save("2", "vless://tiny\n").await.unwrap();
        assert_eq!(paths, vec![dir.path().join("2.txt")]);
        assert_eq!(
            std::fs::read_to_string(&paths[0]).unwrap(),
            "vless://tiny\n"
        );
        for part in parts {
            assert!(!part.exists(), "{} should be removed", part.display());
        }
    }

    #[tokio::test]
    async fn smaller_resplit_removes_tail_parts() {
        let dir = tempfile::tempdir().unwrap();
        let saver = FileSaver::with_max_per_file(dir.path(), 7);

        let first = saver.save("3", &numbered_lines(30)).await.unwrap();
        assert_eq!(first.len(), 5);
        let second = saver.save("3", &numbered_lines(12)).await.unwrap();
        assert_eq!(second.len(), 2);
        for extra in &first[second.len()..] {
            assert!(!extra.exists(), "{} should be removed", extra.display());
        }
        let reassembled: String = second
            .iter()
            .map(std::fs::read_to_string)
            .collect::<std::io::Result<String>>()
            .unwrap();
        assert_eq!(reassembled, numbered_lines(12));
    }
}
