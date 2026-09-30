//! Сквозной тест пайплайна без реальной сети:
//! фейковый Fetcher → настоящий decode → настоящий FileSaver.

use std::collections::HashMap;
use std::path::PathBuf;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use vc::{FileSaver, Source, SyncService};

struct MapFetcher(HashMap<String, String>);

impl vc::Fetcher for MapFetcher {
    async fn fetch(&self, url: &str) -> vc::Result<String> {
        self.0.get(url).cloned().ok_or(vc::Error::BadStatus {
            url: url.to_string(),
            status: 404,
        })
    }
}

#[tokio::test]
async fn pipeline_decodes_and_saves_numbered_files() {
    let plain = "vless://uuid@host:443?security=tls#one\ntrojan://p@host:443#two\n";
    let fetcher = MapFetcher(HashMap::from([
        ("https://example.com/plain".into(), plain.to_string()),
        ("https://example.com/encoded".into(), STANDARD.encode(plain)),
    ]));

    let dir = tempfile::tempdir().unwrap();
    let service = SyncService::new(fetcher, FileSaver::new(dir.path()));
    let sources = vec![
        Source {
            id: "1".into(),
            url: "https://example.com/plain".into(),
        },
        Source {
            id: "2".into(),
            url: "https://example.com/encoded".into(),
        },
    ];

    let mut outcomes = service.sync_all(&sources).await;
    outcomes.sort_by(|a, b| a.id().cmp(b.id()));
    assert!(outcomes.iter().all(|o| o.is_ok()));

    for id in ["1", "2"] {
        let content = std::fs::read_to_string(dir.path().join(format!("{id}.txt"))).unwrap();
        assert_eq!(content, plain);
    }
}

#[tokio::test]
async fn pipeline_filters_ss_vmess_and_saves_string_id() {
    let mixed = "vless://keep@h:443#n\n\
         ss://drop@h:8388#s\n\
         vmess://drop2\n\
         trojan://keep2@h:443#t\n";
    let fetcher = MapFetcher(HashMap::from([(
        "https://example.com/mixed".to_string(),
        mixed.to_string(),
    )]));

    let dir = tempfile::tempdir().unwrap();
    let service = SyncService::new(fetcher, FileSaver::new(dir.path()));
    let sources = vec![Source {
        id: "OpenRay_all".into(),
        url: "https://example.com/mixed".into(),
    }];

    let outcomes = service.sync_all(&sources).await;
    assert_eq!(outcomes.len(), 1);
    match &outcomes[0] {
        vc::Outcome::Synced {
            skipped,
            bytes,
            paths,
            ..
        } => {
            assert_eq!(*skipped, 2);
            assert_eq!(*paths, vec![dir.path().join("OpenRay_all.txt")]);
            assert_eq!(
                *bytes,
                "vless://keep@h:443#n\ntrojan://keep2@h:443#t\n".len()
            );
        }
        vc::Outcome::Failed { error, .. } => panic!("unexpected failure: {error}"),
    }

    let content = std::fs::read_to_string(dir.path().join("OpenRay_all.txt")).unwrap();
    assert_eq!(content, "vless://keep@h:443#n\ntrojan://keep2@h:443#t\n");
}

#[tokio::test]
async fn pipeline_reads_real_urls_json_shape() {
    // urls.json — это объект {"id": "url"}; проверяем загрузку такой формы.
    let dir = tempfile::tempdir().unwrap();
    let urls = dir.path().join("urls.json");
    std::fs::write(
        &urls,
        r#"{"2":"https://example.com/b","1":"https://example.com/a"}"#,
    )
    .unwrap();

    let sources = vc::load_sources(&urls).unwrap();
    assert_eq!(sources.len(), 2);
    assert_eq!(sources[0].id, "1");
    assert_eq!(sources[1].id, "2");

    let _saver = FileSaver::new(PathBuf::from(dir.path()));
}
