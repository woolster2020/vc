use std::path::Path;

use thiserror::Error;

/// Единый тип ошибок приложения.
#[derive(Debug, Error)]
pub enum Error {
    /// Ошибка чтения файла (urls.json, ...).
    #[error("io error for {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    /// Ошибка парсинга JSON.
    #[error("failed to parse {path}: {source}")]
    Parse {
        path: String,
        #[source]
        source: serde_json::Error,
    },

    /// HTTP-ошибка при загрузке подписки.
    #[error("fetch failed for {url}: {source}")]
    Fetch {
        url: String,
        #[source]
        source: reqwest::Error,
    },

    /// Сервер вернул не-2xx статус.
    #[error("bad http status {status} for {url}")]
    BadStatus { url: String, status: u16 },

    /// Ошибка base64-декодирования.
    #[error("base64 decode failed: {0}")]
    Decode(String),

    /// Декодированные байты — не валидный UTF-8.
    #[error("decoded content is not valid utf-8: {0}")]
    NonUtf8(String),

    /// Ошибка сохранения файла.
    #[error("failed to save {path}: {source}")]
    Save {
        path: String,
        #[source]
        source: std::io::Error,
    },

    /// Пустой список источников — фетчить нечего.
    #[error("no sources found in {path}")]
    EmptySources { path: String },
}

/// Удобный алиас результата.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Маппинг std::io::Error с привязкой к пути.
pub(crate) fn io_err(path: &Path, source: std::io::Error) -> Error {
    Error::Io {
        path: path.display().to_string(),
        source,
    }
}
