//! Сквозной тест индекса `urls.txt`: после пайплайна все сохранённые файлы
//! получают по одной CDN-ссылке в `urls.txt` в естественном порядке.

use std::collections::HashMap;

use vc::{write_urls_file, FileSaver, Source, SyncService, CDN_BASE};

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
async fn pipeline_then_index_lists_all_saved_files() {
    let fetcher = MapFetcher(HashMap::from([
        (
            "https://example.com/small".to_string(),
            "vless://a@b:1#x\n".to_string(),
        ),
        ("https://example.com/big".to_string(), numbered_lines(23)),
    ]));

    let dir = tempfile::tempdir().unwrap();
    let service = SyncService::new(fetcher, FileSaver::with_max_per_file(dir.path(), 10));
    let sources = vec![
        Source {
            id: "1".into(),
            url: "https://example.com/small".into(),
        },
        Source {
            id: "2".into(),
            url: "https://example.com/big".into(),
        },
    ];

    let outcomes = service.sync_all(&sources).await;
    assert!(outcomes.iter().all(|o| o.is_ok()));

    let index = write_urls_file(dir.path()).unwrap();
    assert_eq!(index, dir.path().join("urls.txt"));

    let content = std::fs::read_to_string(&index).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    // 1.txt + три части 2-1..2-3.
    let expected = [
        format!("{CDN_BASE}/1.txt"),
        format!("{CDN_BASE}/2-1.txt"),
        format!("{CDN_BASE}/2-2.txt"),
        format!("{CDN_BASE}/2-3.txt"),
    ];
    assert_eq!(
        lines,
        expected.iter().map(String::as_str).collect::<Vec<_>>(),
    );
    // Сам индекс себя не содержит.
    assert!(!content.contains("urls.txt\nurls"));
}

#[tokio::test]
async fn index_example_line_matches_spec() {
    // Пример из ТЗ: output/1-3.txt ->
    // https://cdn.jsdelivr.net/gh/woolster2020/vc@main/output/1-3.txt
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("1-3.txt"), "vless://a@b:1#x\n").unwrap();

    write_urls_file(dir.path()).unwrap();
    let content = std::fs::read_to_string(dir.path().join("urls.txt")).unwrap();
    assert_eq!(
        content,
        "https://cdn.jsdelivr.net/gh/woolster2020/vc@main/output/1-3.txt\n"
    );
}
