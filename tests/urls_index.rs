//! Сквозной тест индексов зеркал: после пайплайна все сохранённые файлы
//! получают по одной прямой ссылке в каждом `*_urls.txt` в естественном порядке.

use std::collections::HashMap;

use vc::{write_mirror_indexes, FileSaver, Source, SyncService, GITHUB_MIRROR, MIRRORS};

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
async fn pipeline_then_indexes_list_all_saved_files() {
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

    let indexes = write_mirror_indexes(dir.path()).unwrap();
    assert_eq!(indexes.len(), MIRRORS.len());

    // 1.txt + три части 2-1..2-3 — в каждом индексе со своей базой.
    let files = ["1.txt", "2-1.txt", "2-2.txt", "2-3.txt"];
    for (index, mirror) in indexes.iter().zip(MIRRORS.iter()) {
        assert_eq!(*index, dir.path().join(mirror.file_name));
        let content = std::fs::read_to_string(index).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(
            lines,
            files
                .iter()
                .map(|f| format!("{}/{f}", mirror.base))
                .collect::<Vec<_>>()
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
        );
        // Ни один индекс не содержит сам себя.
        assert!(!content.contains("urls.txt\nurls"));
    }
}

#[tokio::test]
async fn index_example_lines_match_spec() {
    // Пример из ТЗ для файла `.proxy-1.txt`:
    // github:  https://raw.githubusercontent.com/woolster2020/vc/refs/heads/main/output/.proxy-1.txt
    // gitlab:  https://gitlab.com/woolster2020/vc/-/raw/main/output/.proxy-1.txt
    // codeberg: https://codeberg.org/woolster2020/vc/raw/branch/main/output/.proxy-1.txt
    // gitea:   https://gitea.com/woolster2020/vc/raw/branch/main/output/.proxy-1.txt
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(".proxy-1.txt"), "vless://a@b:1#x\n").unwrap();

    let indexes = write_mirror_indexes(dir.path()).unwrap();
    assert_eq!(indexes.len(), 4);

    let github = std::fs::read_to_string(&indexes[0]).unwrap();
    assert_eq!(
        github,
        "https://raw.githubusercontent.com/woolster2020/vc/refs/heads/main/output/.proxy-1.txt\n"
    );
    assert_eq!(indexes[0], dir.path().join(GITHUB_MIRROR.file_name));

    let gitlab = std::fs::read_to_string(&indexes[1]).unwrap();
    assert_eq!(
        gitlab,
        "https://gitlab.com/woolster2020/vc/-/raw/main/output/.proxy-1.txt\n"
    );

    let codeberg = std::fs::read_to_string(&indexes[2]).unwrap();
    assert_eq!(
        codeberg,
        "https://codeberg.org/woolster2020/vc/raw/branch/main/output/.proxy-1.txt\n"
    );

    let gitea = std::fs::read_to_string(&indexes[3]).unwrap();
    assert_eq!(
        gitea,
        "https://gitea.com/woolster2020/vc/raw/branch/main/output/.proxy-1.txt\n"
    );
}
