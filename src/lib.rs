//! Библиотека VPN-конфиг фетчера.
//!
//! Слои (упрощённая чистая архитектура):
//! - `config` — загрузка `urls.json` (внешний конфиг);
//! - `fetcher` — порт `Fetcher` + HTTP-адаптер на `reqwest`;
//! - `decoder` — доменное правило "нет `://` → base64";
//! - `saver` — порт `Saver` + файловый адаптер;
//! - `service` — use-case: склеивает порты, гоняет всё конкурентно на Tokio;
//! - `error` — единый тип ошибок;
//! - `settings` — загрузка настроек из переменных среды.

pub mod config;
pub mod decoder;
pub mod error;
pub mod fetcher;
pub mod saver;
pub mod service;
pub mod settings;

pub use config::{load_sources, Source};
pub use decoder::decode_if_needed;
pub use error::{Error, Result};
pub use fetcher::{Fetcher, HttpFetcher};
pub use saver::{FileSaver, Saver};
pub use service::{Outcome, SyncService};
pub use settings::Settings;
