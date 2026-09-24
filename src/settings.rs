//! Настройки из переменных окружения (12-factor).
//!
//! - `VC_URLS_PATH` — путь к `urls.json` (по умолчанию `urls.json`);
//! - `VC_OUTPUT_DIR` — каталог для `N.txt` (по умолчанию `output`);
//! - `VC_TIMEOUT_SECS` — таймаут одного HTTP-запроса в секундах (по умолчанию 120).

use std::path::PathBuf;
use std::time::Duration;

const DEFAULT_URLS: &str = "urls.json";
const DEFAULT_OUTPUT: &str = "output";
const DEFAULT_TIMEOUT_SECS: u64 = 120;

/// Настройки приложения.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub urls_path: PathBuf,
    pub output_dir: PathBuf,
    pub timeout: Duration,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            urls_path: PathBuf::from(DEFAULT_URLS),
            output_dir: PathBuf::from(DEFAULT_OUTPUT),
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
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
        s
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
        for v in ["VC_URLS_PATH", "VC_OUTPUT_DIR", "VC_TIMEOUT_SECS"] {
            std::env::remove_var(v);
        }

        let d = Settings::from_env();
        assert_eq!(d, Settings::default());

        std::env::set_var("VC_URLS_PATH", "custom/urls.json");
        std::env::set_var("VC_OUTPUT_DIR", "custom/out");
        std::env::set_var("VC_TIMEOUT_SECS", "45");
        let c = Settings::from_env();
        assert_eq!(c.urls_path, PathBuf::from("custom/urls.json"));
        assert_eq!(c.output_dir, PathBuf::from("custom/out"));
        assert_eq!(c.timeout, Duration::from_secs(45));

        // Мусор в таймауте и пустой токен игнорируются.
        std::env::set_var("VC_TIMEOUT_SECS", "zero");
        let f = Settings::from_env();
        assert_eq!(f.timeout, Duration::from_secs(DEFAULT_TIMEOUT_SECS));

        for v in ["VC_URLS_PATH", "VC_OUTPUT_DIR", "VC_TIMEOUT_SECS"] {
            std::env::remove_var(v);
        }
    }
}
