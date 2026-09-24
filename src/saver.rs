//! Сохранение декодированных конфигов в `output/{id}.txt`.
//!
//! `Saver` — порт (трейт), `FileSaver` — файловый адаптер.

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// Порт сохранения. Возвращает путь записанного файла.
pub trait Saver: Send + Sync {
    /// Сохранить `content` как `{output_dir}/{id}.txt`.
    fn save(
        &self,
        id: &str,
        content: &str,
    ) -> impl std::future::Future<Output = Result<PathBuf>> + Send;
}

/// Файловый сейвер. Директория создаётся лениво при первом `save`.
pub struct FileSaver {
    output_dir: PathBuf,
}

impl FileSaver {
    pub fn new(output_dir: impl Into<PathBuf>) -> Self {
        Self {
            output_dir: output_dir.into(),
        }
    }

    /// Путь файла для `id` без записи.
    pub fn path_for(&self, id: &str) -> PathBuf {
        self.output_dir.join(format!("{id}.txt"))
    }
}

impl Saver for FileSaver {
    async fn save(&self, id: &str, content: &str) -> Result<PathBuf> {
        let path = self.path_for(id);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| parent_path_error(parent, e))?;
        }
        tokio::fs::write(&path, content)
            .await
            .map_err(|e| Error::Save {
                path: path.display().to_string(),
                source: e,
            })?;
        Ok(path)
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

        let path = saver.save("7", "vless://x\n").await.unwrap();
        assert_eq!(path, out.join("7.txt"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "vless://x\n");
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
}
