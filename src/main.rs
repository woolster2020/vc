//! CLI: `vc [urls.json] [output/] [max_per_file]`.
//!
//! Тонкий слой: настройки (env → CLI-аргументы), сборка сервиса из адаптеров,
//! печать итога. Вся логика — в `vc` (lib).
//!
//! Переменные окружения (приоритет ниже, чем у CLI-аргументов):
//! - `VC_URLS_PATH`, `VC_OUTPUT_DIR`, `VC_TIMEOUT_SECS`, `VC_MAX_PER_FILE`.
//!
//! Пример: `cargo run --release -- urls.json output 300`
//! (максимум 300 конфигураций в одном файле, по умолчанию 500).

use std::process::ExitCode;

use vc::{load_sources, write_mirror_indexes, FileSaver, HttpFetcher, Settings, SyncService};

#[tokio::main]
async fn main() -> ExitCode {
    // CLI-аргументы перекрывают env.
    let cli: Vec<String> = std::env::args().skip(1).collect();
    let settings = Settings::from_env().with_cli_args(&cli);
    let urls_path = settings.urls_path.clone();
    let output_dir = settings.output_dir.clone();

    let sources = match load_sources(&urls_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot load {}: {e}", urls_path.display());
            return ExitCode::FAILURE;
        }
    };

    let fetcher = match HttpFetcher::with_timeout(settings.timeout) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("error: cannot build http client: {e}");
            return ExitCode::FAILURE;
        }
    };
    let service = SyncService::new(
        fetcher,
        FileSaver::with_max_per_file(&output_dir, settings.max_per_file),
    );

    println!(
        "fetching {} source(s) from {} (max {} per file) ...",
        sources.len(),
        urls_path.display(),
        settings.max_per_file,
    );
    let mut outcomes = service.sync_all(&sources).await;
    outcomes.sort_by(|a, b| a.id().cmp(b.id()));

    let mut ok = 0usize;
    for outcome in &outcomes {
        match outcome {
            vc::Outcome::Synced {
                id,
                paths,
                was_encoded,
                bytes,
                skipped,
            } => {
                ok += 1;
                let where_saved = if paths.len() == 1 {
                    paths[0].display().to_string()
                } else {
                    format!(
                        "{} part(s): {} .. {}",
                        paths.len(),
                        paths[0].display(),
                        paths[paths.len() - 1].display()
                    )
                };
                let mut meta = format!("{bytes} bytes");
                if *was_encoded {
                    meta.push_str(", base64");
                }
                if *skipped > 0 {
                    meta.push_str(&format!(", skipped {skipped}"));
                }
                println!("ok   {id}: {where_saved} ({meta})");
            }
            vc::Outcome::Failed { id, url, error } => {
                eprintln!("fail {id}: {url}: {error}");
            }
        }
    }
    println!(
        "done: {ok}/{} synced -> {}",
        sources.len(),
        output_dir.display()
    );

    // Индексы прямых ссылок зеркал на все файлы вывода
    // (кроме самих `*_urls.txt`). Пишем всегда, когда хоть один источник
    // успешно синхронизирован, чтобы подписчики могли забрать список одним запросом.
    if ok > 0 {
        match write_mirror_indexes(&output_dir) {
            Ok(indexes) => {
                for index in &indexes {
                    println!("ok   urls: {} (index)", index.display());
                }
            }
            Err(e) => eprintln!("fail *_urls.txt: {e}"),
        }
    }

    if ok == 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
