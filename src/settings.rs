//! Настройки из переменных окружения (12-factor).
//!
//! - `VC_URLS_PATH` — путь к `urls.json` (по умолчанию `urls.json`);
//! - `VC_OUTPUT_DIR` — каталог для `N.txt` (по умолчанию `output`);
//! - `VC_TIMEOUT_SECS` — таймаут одного HTTP-запроса в секундах (по умолчанию 120);
//! - `VC_MAX_PER_FILE` — максимум конфигураций в одном выходном файле
//!   (по умолчанию 500; большие подписки режутся на `{id}-{n}.txt`).
//!
//! CLI-аргументы перекрывают env, позиционно:
//! `vc [urls.json] [output_dir] [max_per_file]`.

use std::path::PathBuf;
use std::time::Duration;

use crate::splitter::DEFAULT_MAX_PER_FILE;

const DEFAULT_URLS: &str = "urls.json";
const DEFAULT_OUTPUT: &str = "output";
const DEFAULT_TIMEOUT_SECS: u64 = 120;

/// Настройки приложения.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub urls_path: PathBuf,
    pub output_dir: PathBuf,
    pub timeout: Duration,
    pub max_per_file: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            urls_path: PathBuf::from(DEFAULT_URLS),
            output_dir: PathBuf::from(DEFAULT_OUTPUT),
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
            max_per_file: DEFAULT_MAX_PER_FILE,
        }
    }
}

impl Settings {
    /// Собрать настройки из окружения поверх значений по умолчанию.
    /// Пустые значения (`""`, пробелы) трактуются как отсутствие.
    pub fn from_env() -> Self {
        let mut s = Self::default();
        if let Some(v) = nonempty("VC_URLS_PATH") {
            s.urls_path = PathBuf::from(v);
        }
        if let Some(v) = nonempty("VC_OUTPUT_DIR") {
            s.output_dir = PathBuf::from(v);
        }
        if let Some(v) = nonempty("VC_TIMEOUT_SECS") {
            if let Ok(secs) = v.parse::<u64>() {
                if secs > 0 {
                    s.timeout = Duration::from_secs(secs);
                }
            }
        }
        if let Some(v) = nonempty("VC_MAX_PER_FILE") {
            if let Ok(n) = v.parse::<usize>() {
                if n > 0 {
                    s.max_per_file = n;
                }
            }
        }
        s
    }

    /// Перекрыть настройки позиционными CLI-аргументами:
    /// `[urls_path] [output_dir] [max_per_file]`.
    /// Пустые и мусорные значения игнорируются.
    pub fn with_cli_args(mut self, args: &[String]) -> Self {
        let mut args = args.iter();
        if let Some(v) = args.next().filter(|v| !v.trim().is_empty()) {
            self.urls_path = PathBuf::from(v);
        }
        if let Some(v) = args.next().filter(|v| !v.trim().is_empty()) {
            self.output_dir = PathBuf::from(v);
        }
        if let Some(v) = args.next() {
            if let Ok(n) = v.trim().parse::<usize>() {
                if n > 0 {
                    self.max_per_file = n;
                }
            }
        }
        self
    }
}

fn nonempty(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Один тест на все env-кейсы: переменные окружения глобальны для процесса,
    // поэтому всё проверяем последовательно внутри одной #[test]-функции.
    #[test]
    fn env_overrides_and_defaults() {
        for v in [
            "VC_URLS_PATH",
            "VC_OUTPUT_DIR",
            "VC_TIMEOUT_SECS",
            "VC_MAX_PER_FILE",
        ] {
            std::env::remove_var(v);
        }

        let d = Settings::from_env();
        assert_eq!(d, Settings::default());
        assert_eq!(d.max_per_file, DEFAULT_MAX_PER_FILE);

        std::env::set_var("VC_URLS_PATH", "custom/urls.json");
        std::env::set_var("VC_OUTPUT_DIR", "custom/out");
        std::env::set_var("VC_TIMEOUT_SECS", "45");
        std::env::set_var("VC_MAX_PER_FILE", "250");
        let c = Settings::from_env();
        assert_eq!(c.urls_path, PathBuf::from("custom/urls.json"));
        assert_eq!(c.output_dir, PathBuf::from("custom/out"));
        assert_eq!(c.timeout, Duration::from_secs(45));
        assert_eq!(c.max_per_file, 250);

        // Мусор в числах игнорируется, нули — тоже.
        std::env::set_var("VC_TIMEOUT_SECS", "zero");
        std::env::set_var("VC_MAX_PER_FILE", "0");
        let f = Settings::from_env();
        assert_eq!(f.timeout, Duration::from_secs(DEFAULT_TIMEOUT_SECS));
        assert_eq!(f.max_per_file, DEFAULT_MAX_PER_FILE);

        for v in [
            "VC_URLS_PATH",
            "VC_OUTPUT_DIR",
            "VC_TIMEOUT_SECS",
            "VC_MAX_PER_FILE",
        ] {
            std::env::remove_var(v);
        }
    }

    #[test]
    fn cli_args_override_positionally() {
        let s = Settings::default().with_cli_args(&[
            "u.json".to_string(),
            "out".to_string(),
            "300".to_string(),
        ]);
        assert_eq!(s.urls_path, PathBuf::from("u.json"));
        assert_eq!(s.output_dir, PathBuf::from("out"));
        assert_eq!(s.max_per_file, 300);
    }

    #[test]
    fn cli_args_ignore_empty_and_garbage() {
        let s = Settings::default().with_cli_args(&[
            "  ".to_string(),
            String::new(),
            "many".to_string(),
        ]);
        assert_eq!(s, Settings::default());

        let s = Settings::default().with_cli_args(&["0".to_string()]);
        assert_eq!(s.urls_path, PathBuf::from("0"));
        assert_eq!(s.max_per_file, DEFAULT_MAX_PER_FILE);
    }
}
