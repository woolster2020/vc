//! Доменное правило: plain vs base64.
//!
//! Если контент содержит `"://"` — это уже готовые URI (`vless://…`,
//! `vmess://…`, …) и возвращается как есть. Иначе весь контент считается
//! base64-подпиской (возможно многострочной) и декодируется.

use base64::engine::general_purpose::{STANDARD, URL_SAFE};
use base64::Engine as _;

use crate::error::{Error, Result};

/// Маркер "это уже URI-список".
const URI_MARKER: &str = "://";

/// Вернуть контент как есть либо base64-декодированным.
///
/// - Пустые/whitespace-only входы возвращаются пустой строкой без ошибки.
/// - Переводы строк и пробелы внутри base64 игнорируются.
/// - Пробуем `STANDARD`, затем `URL_SAFE` алфавит.
pub fn decode_if_needed(raw: &str) -> Result<String> {
    if raw.contains(URI_MARKER) {
        return Ok(raw.to_string());
    }

    let compact: String = raw.chars().filter(|c| !c.is_whitespace()).collect();
    if compact.is_empty() {
        return Ok(String::new());
    }

    let bytes = STANDARD
        .decode(&compact)
        .or_else(|_| URL_SAFE.decode(&compact))
        .map_err(|e| Error::Decode(e.to_string()))?;

    String::from_utf8(bytes).map_err(|e| Error::NonUtf8(e.to_string()))
}

/// Эвристика для отчётов: был ли контент закодирован.
pub fn is_encoded(raw: &str) -> bool {
    !raw.contains(URI_MARKER) && !raw.trim().is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_uris_pass_through() {
        let raw = "vless://uuid@host:443?x=y#name\nvmess://abc\n";
        assert_eq!(decode_if_needed(raw).unwrap(), raw);
    }

    #[test]
    fn base64_decodes_to_uris() {
        let plain = "vless://uuid@host:443#one\ntrojan://pass@host:443#two\n";
        let encoded = STANDARD.encode(plain);
        assert_eq!(decode_if_needed(&encoded).unwrap(), plain);
    }

    #[test]
    fn multiline_base64_is_stripped_of_whitespace() {
        let plain = "ss://abc@host:8388#tag\n";
        let encoded = STANDARD.encode(plain);
        let multiline = format!("{}\n{}\n", &encoded[..8], &encoded[8..]);
        assert_eq!(decode_if_needed(&multiline).unwrap(), plain);
    }

    #[test]
    fn url_safe_alphabet_is_supported() {
        // Ищем ASCII-payload, чей STANDARD-base64 содержит '+' или '/',
        // чтобы URL_SAFE-вариант реально отличался и шёл по fallback-ветке.
        let mut exercised = false;
        for i in 0..256u32 {
            let plain = format!("probe-{i}-~~~>>>payload\n");
            let std = STANDARD.encode(&plain);
            if std.contains('+') || std.contains('/') {
                assert!(!std.contains("://"));
                let url_safe = std.replace('+', "-").replace('/', "_");
                assert_eq!(decode_if_needed(&url_safe).unwrap(), plain);
                exercised = true;
                break;
            }
        }
        assert!(exercised, "no probe produced '+' or '/' in base64");
    }

    #[test]
    fn invalid_base64_errors() {
        assert!(matches!(
            decode_if_needed("!!!not-base64!!!"),
            Err(Error::Decode(_))
        ));
    }

    #[test]
    fn empty_input_returns_empty() {
        assert_eq!(decode_if_needed("  \n ").unwrap(), "");
    }
}
