//! Библиотека VPN-конфиг фетчера.
//!
//! Слои (упрощённая чистая архитектура):
//! - `config` — загрузка `urls.json` (внешний конфиг);
//! - `fetcher` — порт `Fetcher` + HTTP-адаптер на `reqwest`;
//! - `decoder` — доменное правило "нет `://` → base64";
//! - `filter` — доменное правило "без `ss://`/`vmess://`";
//! - `saver` — порт `Saver` + файловый адаптер (со сплитом >500 конфигов на части);
//! - `service` — use-case: склеивает порты, гоняет всё конкурентно на Tokio;
//! - `error` — единый тип ошибок;
//! - `settings` — загрузка настроек из переменных среды.
//! - `splitter` — нарезка файлов больше лимита конфигов на построчные части.
//! - `urls` — индексы зеркал (`github_/gitlab_/codeberg_/gitea_urls.txt`)
//!   с прямыми ссылками на все файлы вывода.

pub mod config;
pub mod decoder;
pub mod error;
pub mod fetcher;
pub mod filter;
pub mod saver;
pub mod service;
pub mod settings;
pub mod splitter;
pub mod urls;

pub use config::{load_sources, Source};
pub use decoder::decode_if_needed;
pub use error::{Error, Result};
pub use fetcher::{Fetcher, HttpFetcher};
pub use filter::{filter_skipped, is_skipped_line, SKIPPED_PROTOCOLS};
pub use saver::{FileSaver, Saver};
pub use service::{Outcome, SyncService};
pub use settings::Settings;
pub use splitter::{
    count_configs, find_large_files, split_content, split_file_if_large, DEFAULT_MAX_PER_FILE,
};
pub use urls::{
    collect_output_files, mirror_url_for, write_mirror_indexes, CODEBERG_MIRROR, GITEA_MIRROR,
    GITHUB_MIRROR, GITLAB_MIRROR, INDEX_FILE_NAMES, LEGACY_URLS_FILE_NAME, MIRRORS,
};
