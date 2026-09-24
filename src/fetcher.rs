//! Загрузка подписок по HTTP.
//!
//! `Fetcher` — порт (трейт), `HttpFetcher` — адаптер на `reqwest`.
//! Трейт позволяет подменять сеть фейком в тестах (DIP из SOLID).

use crate::error::{Error, Result};

/// Порт загрузки текста подписки по URL.
pub trait Fetcher: Send + Sync {
    /// Скачать тело ответа как текст. Не-2xx статус — [`Error::BadStatus`].
    fn fetch(&self, url: &str) -> impl std::future::Future<Output = Result<String>> + Send;
}

/// HTTP-реализация на `reqwest`: случайный UA, таймаут.
pub struct HttpFetcher {
    client: reqwest::Client,
}

/// Таймаут по умолчанию: подписки бывают тяжёлыми, а сеть — медленной.
pub const DEFAULT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

impl HttpFetcher {
    /// Клиент по умолчанию: таймаут [`DEFAULT_TIMEOUT`] и случайный реальный
    /// User-Agent (через `ua_generator`, чтобы снизить риск блокировок по UA).
    pub fn new() -> Result<Self> {
        Self::with_timeout(DEFAULT_TIMEOUT)
    }

    /// Клиент с заданным таймаутом.
    pub fn with_timeout(timeout: std::time::Duration) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .user_agent(ua_generator::ua::spoof_ua())
            .build()
            .map_err(|e| Error::Fetch {
                url: "<client-build>".to_string(),
                source: e,
            })?;
        Ok(Self { client })
    }

    /// Конструктор для тестов с кастомным клиентом.
    pub fn with_client(client: reqwest::Client) -> Self {
        Self { client }
    }
}

impl Default for HttpFetcher {
    fn default() -> Self {
        Self::new().expect("default http client must build")
    }
}

impl Fetcher for HttpFetcher {
    async fn fetch(&self, url: &str) -> Result<String> {
        let req = self.client.get(url);
        let resp = req.send().await.map_err(|e| Error::Fetch {
            url: url.to_string(),
            source: e,
        })?;
        let status = resp.status();
        if !status.is_success() {
            return Err(Error::BadStatus {
                url: url.to_string(),
                status: status.as_u16(),
            });
        }
        resp.text().await.map_err(|e| Error::Fetch {
            url: url.to_string(),
            source: e,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// Минимальный одноразовый HTTP-сервер на std+tokio без лишних зависимостей.
    async fn serve_once(body: &'static str, status: u16) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 4096];
            let _ = sock.read(&mut buf).await;
            let reason = if status == 200 { "OK" } else { "Not Found" };
            let resp = format!(
                "HTTP/1.1 {status} {reason}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = sock.write_all(resp.as_bytes()).await;
        });
        format!("http://{addr}/sub.txt")
    }

    #[tokio::test]
    async fn fetches_body_on_200() {
        let url = serve_once("vless://a@b:443#x\n", 200).await;
        let body = HttpFetcher::default().fetch(&url).await.unwrap();
        assert_eq!(body, "vless://a@b:443#x\n");
    }

    #[tokio::test]
    async fn rejects_non_2xx() {
        let url = serve_once("nope", 404).await;
        let err = HttpFetcher::default().fetch(&url).await.unwrap_err();
        assert!(matches!(err, Error::BadStatus { status: 404, .. }));
    }
}
