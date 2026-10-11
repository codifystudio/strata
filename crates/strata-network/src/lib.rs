//! Strata network — асинхронный HTTP(S) клиент с поддержкой редиректов, gzip, brotli.

use std::time::Duration;

use reqwest::{Client, Response};
use strata_core::{StrataError, StrataResult};
use url::Url;

pub struct FetchResult {
    pub final_url: Url,
    pub status: u16,
    pub content_type: String,
    pub bytes: Vec<u8>,
}

pub struct NetworkClient {
    http: Client,
}

impl NetworkClient {
    pub fn new() -> StrataResult<Self> {
        let http = Client::builder()
            .user_agent("Mozilla/5.0 (compatible; Strata/0.1; +https://github.com/codifystudio/strata)")
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()
            .map_err(|e| StrataError::Network(e.to_string()))?;

        Ok(Self { http })
    }

    pub async fn fetch(&self, url: &Url) -> StrataResult<FetchResult> {
        log::debug!("fetch: {}", url);

        let resp: Response = self
            .http
            .get(url.as_str())
            .send()
            .await
            .map_err(|e| StrataError::Network(e.to_string()))?;

        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            return Err(StrataError::Network(format!(
                "HTTP {} for {}",
                status,
                url
            )));
        }

        let final_url = resp.url().clone();
        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_string();

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| StrataError::Network(e.to_string()))?
            .to_vec();

        log::debug!(
            "fetched: {} bytes from {} (status {}, content-type {})",
            bytes.len(),
            final_url,
            status,
            content_type
        );

        Ok(FetchResult {
            final_url,
            status,
            content_type,
            bytes,
        })
    }
}

impl Default for NetworkClient {
    fn default() -> Self {
        Self::new().expect("failed to build NetworkClient")
    }
}
