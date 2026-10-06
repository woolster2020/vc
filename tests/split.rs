//! Сквозной тест сплита: большой пайплайн режется на части `{id}-{n}.txt`
//! по лимиту конфигураций, исходный `output/{id}.txt` удаляется,
//! сборка частей даёт исходник.

use std::collections::HashMap;

use vc::{count_configs, FileSaver, Source, SyncService};

struct MapFetcher(HashMap<String, String>);

impl vc::Fetcher for MapFetcher {
    async fn fetch(&self, url: &str) -> vc::Result<String> {
        self.0.get(url).cloned().ok_or(vc::Error::BadStatus {
            url: url.to_string(),
            status: 404,
        })
    }
}

fn numbered_lines(count: usize) -> String {
    (0..count)
        .map(|i| format!("vless://user{i}@host:443#node-{i}\n"))
        .collect()
}

#[tokio::test]
async fn pipeline_splits_large_subscription_into_parts() {
    let plain = numbered_lines(23);
    let fetcher = MapFetcher(HashMap::from([(
        "https://example.com/big".to_string(),
        plain.clone(),
    )]));

    let dir = tempfile::tempdir().unwrap();
    // Маленький лимит, чтобы не гонять сотни конфигов в тесте.
    let service = SyncService::new(fetcher, FileSaver::with_max_per_file(dir.path(), 10));
    let sources = vec![Source {
        id: "1".into(),
        url: "https://example.com/big".into(),
    }];

    let outcomes = service.sync_all(&sources).await;
    assert_eq!(outcomes.len(), 1);
    assert!(outcomes[0].is_ok());

    // Исходного большого файла быть не должно — только части.
    assert!(!dir.path().join("1.txt").exists());

    let mut parts: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    parts.sort();
    assert_eq!(parts.len(), 3);
    assert_eq!(parts[0], dir.path().join("1-1.txt"));

    // 10 + 10 + 3 (последний — сколько останется).
    let counts: Vec<usize> = parts
        .iter()
        .map(|p| count_configs(&std::fs::read_to_string(p).unwrap()))
        .collect();
    assert_eq!(counts, vec![10, 10, 3]);

    let reassembled: String = parts
        .iter()
        .map(std::fs::read_to_string)
        .collect::<std::io::Result<String>>()
        .unwrap();
    assert_eq!(reassembled, plain);

    // Outcome сообщает о всех частях.
    match &outcomes[0] {
        vc::Outcome::Synced { paths, bytes, .. } => {
            assert_eq!(*bytes, plain.len());
            assert_eq!(*paths, parts);
        }
        vc::Outcome::Failed { error, .. } => panic!("unexpected failure: {error}"),
    }
}

#[tokio::test]
async fn pipeline_splits_string_id_without_skipped_protocols() {
    // 28 строк: каждая 6-я (i = 5, 11, 17, 23) — ss:// под отсев.
    let mut mixed = String::new();
    let mut expected = String::new();
    for i in 0..28 {
        if i % 6 == 5 {
            mixed.push_str(&format!("ss://drop{i}@host:8388#s{i}\n"));
        } else {
            let line = format!("vless://user{i}@host:443#node-{i}\n");
            mixed.push_str(&line);
            expected.push_str(&line);
        }
    }

    let fetcher = MapFetcher(HashMap::from([(
        "https://example.com/mixed-big".to_string(),
        mixed,
    )]));

    let dir = tempfile::tempdir().unwrap();
    let service = SyncService::new(fetcher, FileSaver::with_max_per_file(dir.path(), 10));
    let sources = vec![Source {
        id: "OpenRay_all".into(),
        url: "https://example.com/mixed-big".into(),
    }];

    let outcomes = service.sync_all(&sources).await;
    assert_eq!(outcomes.len(), 1);
    match &outcomes[0] {
        vc::Outcome::Synced { skipped, .. } => assert_eq!(*skipped, 4),
        vc::Outcome::Failed { error, .. } => panic!("unexpected failure: {error}"),
    }

    // После отсева 24 конфига → части 10 + 10 + 4.
    let mut parts: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    parts.sort();
    assert_eq!(parts.len(), 3);
    assert_eq!(parts[0], dir.path().join("OpenRay_all-1.txt"));
    assert_eq!(parts[2], dir.path().join("OpenRay_all-3.txt"));

    let counts: Vec<usize> = parts
        .iter()
        .map(|p| count_configs(&std::fs::read_to_string(p).unwrap()))
        .collect();
    assert_eq!(counts, vec![10, 10, 4]);

    let reassembled: String = parts
        .iter()
        .map(std::fs::read_to_string)
        .collect::<std::io::Result<String>>()
        .unwrap();
    assert_eq!(reassembled, expected);
    assert!(reassembled.lines().all(|l| !l.starts_with("ss://")));
}

#[tokio::test]
async fn pipeline_fills_parts_to_limit_despite_heavy_filtering() {
    // Регрессия на жалобу «OpenRay_all-1.txt короче 500»:
    // фильтр по протоколу обязан идти ДО нарезки по количеству,
    // иначе первые части недобираются. 1200 строк, каждая 3-я —
    // ss:// или vmess:// под отсев (400 шт), лимит 500.
    // После отсева 800 конфигов → части ровно 500 + 300.
    let mut mixed = String::new();
    let mut expected = String::new();
    for i in 0..1200 {
        if i % 3 == 2 {
            if i % 2 == 0 {
                mixed.push_str(&format!("ss://drop{i}@host:8388#s{i}\n"));
            } else {
                mixed.push_str(&format!("vmess://drop{i}\n"));
            }
        } else {
            let line = format!("vless://user{i}@host:443#node-{i}\n");
            mixed.push_str(&line);
            expected.push_str(&line);
        }
    }

    let fetcher = MapFetcher(HashMap::from([(
        "https://example.com/heavy".to_string(),
        mixed,
    )]));

    let dir = tempfile::tempdir().unwrap();
    let service = SyncService::new(fetcher, FileSaver::with_max_per_file(dir.path(), 500));
    let sources = vec![Source {
        id: "OpenRay_all".into(),
        url: "https://example.com/heavy".into(),
    }];

    let outcomes = service.sync_all(&sources).await;
    assert_eq!(outcomes.len(), 1);
    match &outcomes[0] {
        vc::Outcome::Synced { skipped, .. } => assert_eq!(*skipped, 400),
        vc::Outcome::Failed { error, .. } => panic!("unexpected failure: {error}"),
    }

    let mut parts: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    parts.sort();
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0], dir.path().join("OpenRay_all-1.txt"));
    assert_eq!(parts[1], dir.path().join("OpenRay_all-2.txt"));

    // Первая часть — ровно 500, несмотря на 400 выброшенных строк.
    let counts: Vec<usize> = parts
        .iter()
        .map(|p| count_configs(&std::fs::read_to_string(p).unwrap()))
        .collect();
    assert_eq!(counts, vec![500, 300]);

    for part in &parts {
        let body = std::fs::read_to_string(part).unwrap();
        assert!(
            body.lines()
                .all(|l| !l.starts_with("ss://") && !l.starts_with("vmess://")),
            "{} must not contain skipped protocols",
            part.display()
        );
    }

    let reassembled: String = parts
        .iter()
        .map(std::fs::read_to_string)
        .collect::<std::io::Result<String>>()
        .unwrap();
    assert_eq!(reassembled, expected);
}

#[tokio::test]
async fn pipeline_keeps_small_subscription_as_single_file() {
    let plain = "vless://uuid@host:443#one\n";
    let fetcher = MapFetcher(HashMap::from([(
        "https://example.com/small".to_string(),
        plain.to_string(),
    )]));

    let dir = tempfile::tempdir().unwrap();
    let service = SyncService::new(fetcher, FileSaver::with_max_per_file(dir.path(), 10));
    let sources = vec![Source {
        id: "5".into(),
        url: "https://example.com/small".into(),
    }];

    let outcomes = service.sync_all(&sources).await;
    assert!(outcomes[0].is_ok());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("5.txt")).unwrap(),
        plain
    );
    assert!(!dir.path().join("5-1.txt").exists());
}
