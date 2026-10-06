//! Use-case синхронизации: fetch → decode → filter → save для всех источников.
//!
//! Порядок важен: фильтр протоколов идёт ДО подсчёта и нарезки,
//! поэтому непоследние части `{id}-{n}.txt` всегда полные
//! (ровно `max_per_file` конфигов), а не «дырявые» после отсева.
//!
//! Каждый источник обрабатывается в собственной Tokio-задаче
//! (конкурентные HTTP-запросы). Ошибка одного источника не отменяет
//! остальные — она возвращается в [`Outcome::Failed`].

use std::path::PathBuf;
use std::sync::Arc;

use tokio::task::JoinSet;

use crate::config::Source;
use crate::decoder::{decode_if_needed, is_encoded};
use crate::error::Error;
use crate::fetcher::Fetcher;
use crate::filter::filter_content;
use crate::saver::Saver;

/// Результат обработки одного источника.
#[derive(Debug)]
pub enum Outcome {
    /// Успех: что сохранили и был ли контент закодирован.
    /// `paths` — один файл либо несколько частей `{id}-{n}.txt` для
    /// больших подписок (см. [`crate::splitter`]).
    /// `skipped` — сколько строк `ss://`/`vmess://` выброшено,
    /// `duplicates` — сколько точных дублей схлопнуто,
    /// `invalid` — сколько строк без `://` отброшено
    /// (см. [`crate::filter`]).
    Synced {
        id: String,
        paths: Vec<PathBuf>,
        was_encoded: bool,
        bytes: usize,
        skipped: usize,
        duplicates: usize,
        invalid: usize,
    },
    /// Ошибка: какой источник и что случилось.
    Failed {
        id: String,
        url: String,
        error: Error,
    },
}

impl Outcome {
    pub fn is_ok(&self) -> bool {
        matches!(self, Outcome::Synced { .. })
    }

    pub fn id(&self) -> &str {
        match self {
            Outcome::Synced { id, .. } => id,
            Outcome::Failed { id, .. } => id,
        }
    }
}

/// Сервис синхронизации, параметризованный портами `Fetcher`/`Saver`
/// (инверсия зависимостей: бизнес-логика не знает про reqwest и fs).
pub struct SyncService<F, S> {
    fetcher: Arc<F>,
    saver: Arc<S>,
}

impl<F, S> SyncService<F, S>
where
    F: Fetcher + 'static,
    S: Saver + 'static,
{
    pub fn new(fetcher: F, saver: S) -> Self {
        Self {
            fetcher: Arc::new(fetcher),
            saver: Arc::new(saver),
        }
    }

    /// Обработать один источник: скачать, декодировать при необходимости,
    /// выбросить `ss://`/`vmess://`, дубли и мусор без `://`, сохранить.
    pub async fn sync_one(&self, source: &Source) -> Outcome {
        let raw = match self.fetcher.fetch(&source.url).await {
            Ok(raw) => raw,
            Err(error) => {
                return Outcome::Failed {
                    id: source.id.clone(),
                    url: source.url.clone(),
                    error,
                };
            }
        };

        let was_encoded = is_encoded(&raw);
        let decoded = match decode_if_needed(&raw) {
            Ok(decoded) => decoded,
            Err(error) => {
                return Outcome::Failed {
                    id: source.id.clone(),
                    url: source.url.clone(),
                    error,
                };
            }
        };

        let (filtered, stats) = filter_content(&decoded);

        match self.saver.save(&source.id, &filtered).await {
            Ok(paths) => Outcome::Synced {
                id: source.id.clone(),
                paths,
                was_encoded,
                bytes: filtered.len(),
                skipped: stats.skipped,
                duplicates: stats.duplicates,
                invalid: stats.invalid,
            },
            Err(error) => Outcome::Failed {
                id: source.id.clone(),
                url: source.url.clone(),
                error,
            },
        }
    }

    /// Обработать все источники конкурентно. Порядок результатов —
    /// порядок завершения задач; вызывающий может отсортировать по `id`.
    pub async fn sync_all(&self, sources: &[Source]) -> Vec<Outcome> {
        // Клонируем: спавнящиеся задачи требуют 'static.
        let owned = sources.to_vec();
        let mut set = JoinSet::new();
        for source in owned {
            let fetcher = Arc::clone(&self.fetcher);
            let saver = Arc::clone(&self.saver);
            set.spawn(async move {
                let svc = SyncService { fetcher, saver };
                svc.sync_one(&source).await
            });
        }

        let mut outcomes = Vec::with_capacity(sources.len());
        while let Some(done) = set.join_next().await {
            match done {
                Ok(outcome) => outcomes.push(outcome),
                Err(join_err) => outcomes.push(Outcome::Failed {
                    id: "<task>".to_string(),
                    url: String::new(),
                    error: Error::Decode(format!("task panicked: {join_err}")),
                }),
            }
        }
        outcomes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine as _;
    use std::collections::HashMap;
    use std::sync::Mutex;

    struct FakeFetcher {
        bodies: HashMap<String, String>,
    }

    impl Fetcher for FakeFetcher {
        async fn fetch(&self, url: &str) -> crate::error::Result<String> {
            self.bodies.get(url).cloned().ok_or(Error::BadStatus {
                url: url.to_string(),
                status: 404,
            })
        }
    }

    struct FakeSaver {
        stored: Mutex<HashMap<String, String>>,
    }

    impl FakeSaver {
        fn new() -> Self {
            Self {
                stored: Mutex::new(HashMap::new()),
            }
        }
    }

    impl Saver for FakeSaver {
        async fn save(&self, id: &str, content: &str) -> crate::error::Result<Vec<PathBuf>> {
            self.stored
                .lock()
                .unwrap()
                .insert(id.to_string(), content.to_string());
            Ok(vec![PathBuf::from(format!("output/{id}.txt"))])
        }
    }

    fn sources() -> Vec<Source> {
        vec![
            Source {
                id: "1".into(),
                url: "https://x/1".into(),
            },
            Source {
                id: "2".into(),
                url: "https://x/2".into(),
            },
            Source {
                id: "3".into(),
                url: "https://x/missing".into(),
            },
        ]
    }

    #[tokio::test]
    async fn syncs_plain_and_base64_and_reports_failure() {
        use base64::engine::general_purpose::STANDARD;
        let plain = "vless://u@h:443#n\n";
        let fetcher = FakeFetcher {
            bodies: HashMap::from([
                ("https://x/1".to_string(), plain.to_string()),
                ("https://x/2".to_string(), STANDARD.encode(plain)),
            ]),
        };
        let saver = FakeSaver::new();
        let svc = SyncService::new(fetcher, saver);

        let mut outcomes = svc.sync_all(&sources()).await;
        outcomes.sort_by(|a, b| a.id().cmp(b.id()));

        assert!(outcomes[0].is_ok());
        assert!(outcomes[1].is_ok());
        assert!(!outcomes[2].is_ok());

        let stored = svc.saver.stored.lock().unwrap();
        assert_eq!(stored.get("1").unwrap(), plain);
        assert_eq!(stored.get("2").unwrap(), plain);
        assert!(!stored.contains_key("3"));
    }

    #[tokio::test]
    async fn drops_ss_and_vmess_lines_before_save() {
        let mixed = "vless://keep@h:443#n\n\
             ss://drop@h:8388#s\n\
             vmess://drop2\n\
             trojan://keep2@h:443#t\n";
        let fetcher = FakeFetcher {
            bodies: HashMap::from([("https://x/1".to_string(), mixed.to_string())]),
        };
        let svc = SyncService::new(fetcher, FakeSaver::new());
        let outcomes = svc
            .sync_all(&[Source {
                id: "OpenRay_all".into(),
                url: "https://x/1".into(),
            }])
            .await;
        assert_eq!(outcomes.len(), 1);

        match &outcomes[0] {
            Outcome::Synced { skipped, .. } => assert_eq!(*skipped, 2),
            Outcome::Failed { error, .. } => panic!("unexpected failure: {error}"),
        }

        let stored = svc.saver.stored.lock().unwrap();
        assert_eq!(
            stored.get("OpenRay_all").unwrap(),
            "vless://keep@h:443#n\ntrojan://keep2@h:443#t\n"
        );
    }

    #[tokio::test]
    async fn dedupes_exact_lines_and_drops_invalid_before_save() {
        let messy = "vless://keep@h:443#n\n\
             vless://keep@h:443#n\n\
             just-a-remark-without-scheme\n\
             trojan://keep2@h:443#t\n";
        let fetcher = FakeFetcher {
            bodies: HashMap::from([("https://x/9".to_string(), messy.to_string())]),
        };
        let svc = SyncService::new(fetcher, FakeSaver::new());
        let outcomes = svc
            .sync_all(&[Source {
                id: "dupes".into(),
                url: "https://x/9".into(),
            }])
            .await;
        assert_eq!(outcomes.len(), 1);

        match &outcomes[0] {
            Outcome::Synced {
                skipped,
                duplicates,
                invalid,
                ..
            } => {
                assert_eq!(*skipped, 0);
                assert_eq!(*duplicates, 1);
                assert_eq!(*invalid, 1);
            }
            Outcome::Failed { error, .. } => panic!("unexpected failure: {error}"),
        }

        let stored = svc.saver.stored.lock().unwrap();
        assert_eq!(
            stored.get("dupes").unwrap(),
            "vless://keep@h:443#n\ntrojan://keep2@h:443#t\n"
        );
    }

    #[tokio::test]
    async fn decode_error_becomes_failed_outcome() {
        let fetcher = FakeFetcher {
            bodies: HashMap::from([("https://x/1".to_string(), "!!!not-base64!!!".to_string())]),
        };
        let svc = SyncService::new(fetcher, FakeSaver::new());
        let outcomes = svc
            .sync_all(&[Source {
                id: "1".into(),
                url: "https://x/1".into(),
            }])
            .await;
        assert!(!outcomes[0].is_ok());
    }
}
