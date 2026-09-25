//! CLI: `vc [urls.json] [output/]`.
//!
//! Тонкий слой: настройки (env → CLI-аргументы), сборка сервиса из адаптеров,
//! печать итога. Вся логика — в `vc` (lib).
//!
//! Переменные окружения (приоритет ниже, чем у CLI-аргументов):
//! - `VC_URLS_PATH`, `VC_OUTPUT_DIR`, `VC_TIMEOUT_SECS`.

use std::process::ExitCode;

use vc::{load_sources, FileSaver, HttpFetcher, Settings, SyncService};

#[tokio::main]
async fn main() -> ExitCode {
    let settings = Settings::from_env();

    // CLI-аргументы перекрывают env.
    let mut args = std::env::args().skip(1);
    let urls_path = args.next().map_or(settings.urls_path.clone(), Into::into);
    let output_dir = args.next().map_or(settings.output_dir.clone(), Into::into);

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
    let service = SyncService::new(fetcher, FileSaver::new(&output_dir));

    println!(
        "fetching {} source(s) from {} ...",
        sources.len(),
        urls_path.display()
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
                println!(
                    "ok   {id}: {where_saved} ({} bytes{})",
                    bytes,
                    if *was_encoded { ", base64" } else { "" }
                );
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

    if ok == 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
