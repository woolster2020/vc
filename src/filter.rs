//! Отсев нежелательных протоколов.
//!
//! Строки, начинающиеся с одного из [`SKIPPED_PROTOCOLS`] (`ss://`, `vmess://`),
//! не сохраняются в выходные файлы. Проверка — построчно, по префиксу
//! после обрезки ведущих пробелов; регистр учитывается (в подписках
//! схемы всегда в нижнем регистре).

/// Протоколы, которые не сохраняем в выходные файлы.
pub const SKIPPED_PROTOCOLS: &[&str] = &["ss://", "vmess://"];

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
}
