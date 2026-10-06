//! Отсев нежелательных строк.
//!
//! В выходные файлы не сохраняются:
//! - строки с нежелательными протоколами ([`SKIPPED_PROTOCOLS`]: `ss://`, `vmess://`);
//! - точные дубли строк (первое вхождение сохраняется);
//! - непустые строки без `://` (мусор вроде обломков многострочных записей,
//!   который ни один клиент не распарсит).
//!
//! Проверка — построчно, пустые строки проходят как есть.
//! Сравнение протоколов — по префиксу после обрезки ведущих пробелов,
//! регистр учитывается (в подписках схемы всегда в нижнем регистре).

use std::collections::HashSet;

/// Протоколы, которые не сохраняем в выходные файлы.
pub const SKIPPED_PROTOCOLS: &[&str] = &["ss://", "vmess://"];

/// Статистика построчной чистки (см. [`filter_content`]).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct FilterStats {
    /// Строки с протоколами из [`SKIPPED_PROTOCOLS`].
    pub skipped: usize,
    /// Точные дубли строк (первое вхождение сохранено).
    pub duplicates: usize,
    /// Непустые строки без `://`.
    pub invalid: usize,
}

impl FilterStats {
    /// Сколько строк всего выброшено.
    pub fn total_removed(&self) -> usize {
        self.skipped + self.duplicates + self.invalid
    }
}

/// Проверить, начинается ли строка с одного из пропускаемых протоколов.
pub fn is_skipped_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    SKIPPED_PROTOCOLS
        .iter()
        .any(|prefix| trimmed.starts_with(prefix))
}

/// Выбросить пропускаемые строки, остальные вернуть байт-в-байт.
///
/// Возвращает `(отфильтрованный_контент, число_выброшенных_строк)`.
/// Пустые строки и окончания строк (`\n`, `\r\n`, их отсутствие
/// в конце файла) сохраняются как есть.
pub fn filter_skipped(content: &str) -> (String, usize) {
    let mut kept = String::new();
    let mut skipped = 0usize;
    // `split_inclusive` сохраняет оригинальные окончания строк,
    // поэтому kept-часть собирается точно.
    for line in content.split_inclusive('\n') {
        if is_skipped_line(line) {
            skipped += 1;
        } else {
            kept.push_str(line);
        }
    }
    (kept, skipped)
}

/// Почистить контент: отсев протоколов, точных дублей и мусора без `://`.
///
/// Порядок строк сохраняется, первое вхождение дублирующейся строки
/// сохраняется байт-в-байт (сравнение — по обрезанной строке, так что
/// дубли с висячими пробелами тоже схлопываются).
/// Пустые строки проходят как есть и ни в какие счётчики не попадают.
pub fn filter_content(content: &str) -> (String, FilterStats) {
    let (without_protocols, skipped) = filter_skipped(content);
    let mut stats = FilterStats {
        skipped,
        ..Default::default()
    };

    let mut kept = String::new();
    let mut seen = HashSet::new();
    for line in without_protocols.split_inclusive('\n') {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            kept.push_str(line);
        } else if !trimmed.contains("://") {
            stats.invalid += 1;
        } else if !seen.insert(trimmed.to_string()) {
            stats.duplicates += 1;
        } else {
            kept.push_str(line);
        }
    }
    (kept, stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_skipped_protocols() {
        for line in [
            "ss://abc@host:8388#tag\n",
            "vmess://eyJhZGQiOiIx\n",
            "   ss://indented@host:1#x\n",
        ] {
            assert!(is_skipped_line(line), "{line:?} should be skipped");
        }
    }

    #[test]
    fn keeps_other_protocols_and_blank_lines() {
        for line in [
            "vless://uuid@host:443#n\n",
            "trojan://p@host:443#t\n",
            "hysteria2://u@host:443#h\n",
            "\n",
            "   \n",
            "",
        ] {
            assert!(!is_skipped_line(line), "{line:?} should be kept");
        }
    }

    #[test]
    fn prefix_only_at_start_counts() {
        // Вхождение протокола не в начале строки — не повод для отсева.
        assert!(!is_skipped_line("vless://u@h:443#see-ss://inside\n"));
    }

    #[test]
    fn filter_drops_skipped_and_reports_count() {
        let mixed = "vless://keep@h:443#n\n\
             ss://drop@h:8388#s\n\
             vmess://drop2\n\
             trojan://keep2@h:443#t\n";
        let (kept, skipped) = filter_skipped(mixed);
        assert_eq!(kept, "vless://keep@h:443#n\ntrojan://keep2@h:443#t\n");
        assert_eq!(skipped, 2);
    }

    #[test]
    fn filter_preserves_missing_trailing_newline() {
        let (kept, skipped) = filter_skipped("ss://drop@h:1#x\nvless://keep@h:1#y");
        assert_eq!(kept, "vless://keep@h:1#y");
        assert!(!kept.ends_with('\n'));
        assert_eq!(skipped, 1);
    }

    #[test]
    fn filter_empty_input_is_empty() {
        assert_eq!(filter_skipped(""), (String::new(), 0));
    }

    #[test]
    fn filter_content_dedupes_and_drops_invalid() {
        let mixed = "vless://keep@h:443#n\n\
             ss://drop@h:8388#s\n\
             vless://keep@h:443#n\n\
             just-a-remark-without-scheme\n\
             trojan://keep2@h:443#t \n\
             trojan://keep2@h:443#t\n\
             \n\
             vmess://drop2\n";
        let (kept, stats) = filter_content(mixed);
        // Первое вхождение сохраняется байт-в-байт (с висячим пробелом),
        // повтор без пробела — дубль. Пустая строка проходит как есть.
        assert_eq!(kept, "vless://keep@h:443#n\ntrojan://keep2@h:443#t \n\n");
        assert_eq!(
            stats,
            FilterStats {
                skipped: 2,
                duplicates: 2,
                invalid: 1,
            }
        );
        assert_eq!(stats.total_removed(), 5);
    }

    #[test]
    fn filter_content_keeps_unique_without_scheme_noise() {
        // Без дублей и мусора статистика нулевая, контент тот же.
        let plain = "vless://a@h:1#x\ntrojan://b@h:2#y\n";
        let (kept, stats) = filter_content(plain);
        assert_eq!(kept, plain);
        assert_eq!(stats, FilterStats::default());
    }
}
