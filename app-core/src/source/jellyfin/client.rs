//! Thin, reusable Jellyfin HTTP client.
//!
//! Wraps a single `ureq::Agent` with sensible timeouts and centralises the
//! `X-Emby-Authorization` / `X-Emby-Token` headers so every call site stops
//! re-deriving them. All errors get wrapped in `NightingaleError::Jellyfin
//! { stage, source }` so the user-facing message format is consistent
//! regardless of which endpoint failed.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use serde::de::DeserializeOwned;
use ureq::Agent;

use crate::error::NightingaleError;
use crate::source::StreamResponse;

const CLIENT_NAME: &str = "Nightingale";
const CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Identifies this install to Jellyfin. The `token` is the only auth scrap
/// passed via headers; if it's `None` (login flow) the `X-Emby-Token` header
/// is omitted.
#[derive(Clone)]
pub struct AuthHeaders {
    device_id: Arc<str>,
    token: Option<Arc<str>>,
}

impl AuthHeaders {
    pub fn for_token(token: impl Into<String>, device_id: impl Into<String>) -> Self {
        Self {
            device_id: Arc::from(device_id.into()),
            token: Some(Arc::from(token.into())),
        }
    }

    pub fn anonymous(device_id: impl Into<String>) -> Self {
        Self {
            device_id: Arc::from(device_id.into()),
            token: None,
        }
    }

    fn authorization_value(&self) -> String {
        match self.token.as_deref() {
            Some(token) => format!(
                "MediaBrowser Client=\"{}\", Device=\"{}\", DeviceId=\"{}\", Version=\"{}\", Token=\"{}\"",
                CLIENT_NAME, CLIENT_NAME, self.device_id, CLIENT_VERSION, token
            ),
            None => format!(
                "MediaBrowser Client=\"{}\", Device=\"{}\", DeviceId=\"{}\", Version=\"{}\"",
                CLIENT_NAME, CLIENT_NAME, self.device_id, CLIENT_VERSION
            ),
        }
    }

    pub fn token(&self) -> Option<&str> {
        self.token.as_deref()
    }
}

#[derive(Clone)]
pub struct JellyfinClient {
    agent: Agent,
    base_url: String,
    headers: AuthHeaders,
}

impl JellyfinClient {
    pub fn new(base_url: impl Into<String>, headers: AuthHeaders) -> Self {
        let config = Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(10)))
            .timeout_recv_response(Some(Duration::from_secs(30)))
            .build();
        let agent = Agent::new_with_config(config);
        Self {
            agent,
            base_url: trim_base_url(&base_url.into()),
            headers,
        }
    }

    /// Variant without a read-response timeout — used for streaming downloads
    /// where the request itself may take arbitrarily long but bytes arrive
    /// continuously.
    pub fn for_downloads(base_url: impl Into<String>, headers: AuthHeaders) -> Self {
        let config = Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(10)))
            .build();
        let agent = Agent::new_with_config(config);
        Self {
            agent,
            base_url: trim_base_url(&base_url.into()),
            headers,
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn headers(&self) -> &AuthHeaders {
        &self.headers
    }

    fn endpoint(&self, path: &str) -> String {
        let trimmed = path.trim_start_matches('/');
        format!("{}/{}", self.base_url, trimmed)
    }

    fn authed_get(
        &self,
        path: &str,
        query: &[(&str, &str)],
    ) -> ureq::RequestBuilder<ureq::typestate::WithoutBody> {
        let mut req = self
            .agent
            .get(self.endpoint(path))
            .header("X-Emby-Authorization", self.headers.authorization_value())
            .header("Accept", "application/json");
        if let Some(token) = self.headers.token() {
            req = req.header("X-Emby-Token", token);
        }
        for (k, v) in query {
            req = req.query(*k, *v);
        }
        req
    }

    pub fn get_json<T: DeserializeOwned>(
        &self,
        stage: &'static str,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<T, NightingaleError> {
        let resp = self
            .authed_get(path, query)
            .call()
            .map_err(|e| NightingaleError::jellyfin(stage, e))?;
        resp.into_body()
            .read_json()
            .map_err(|e| NightingaleError::jellyfin(stage, e))
    }

    pub fn post_json<B: Serialize, T: DeserializeOwned>(
        &self,
        stage: &'static str,
        path: &str,
        body: &B,
    ) -> Result<T, NightingaleError> {
        let mut req = self
            .agent
            .post(self.endpoint(path))
            .header("X-Emby-Authorization", self.headers.authorization_value())
            .header("Accept", "application/json")
            .header("Content-Type", "application/json");
        if let Some(token) = self.headers.token() {
            req = req.header("X-Emby-Token", token);
        }
        let resp = req
            .send_json(body)
            .map_err(|e| NightingaleError::jellyfin(stage, e))?;
        resp.into_body()
            .read_json()
            .map_err(|e| NightingaleError::jellyfin(stage, e))
    }

    pub fn download_to_vec(
        &self,
        stage: &'static str,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<Vec<u8>, NightingaleError> {
        let resp = self
            .authed_get(path, query)
            .call()
            .map_err(|e| NightingaleError::jellyfin(stage, e))?;
        let mut body = resp.into_body();
        let mut reader = body.as_reader();
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes)?;
        Ok(bytes)
    }

    /// Stream a download to disk, writing to `<dest>.part` first and renaming
    /// on success so a crashed download never leaves a corrupt cache file.
    pub fn download_to_file(
        &self,
        stage: &'static str,
        path: &str,
        query: &[(&str, &str)],
        dest: &Path,
    ) -> Result<PathBuf, NightingaleError> {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let resp = self
            .authed_get(path, query)
            .call()
            .map_err(|e| NightingaleError::jellyfin(stage, e))?;
        let tmp = dest.with_extension("part");
        {
            let mut body = resp.into_body();
            let mut reader = body.as_reader();
            let mut file = std::fs::File::create(&tmp)?;
            std::io::copy(&mut reader, &mut file)?;
            file.flush()?;
        }
        std::fs::rename(&tmp, dest)?;
        Ok(dest.to_path_buf())
    }

    /// Open a streaming response — used by the local media proxy when relaying
    /// Jellyfin video bytes to the renderer. Caller may forward a `Range`
    /// header.
    pub fn open_stream(
        &self,
        stage: &'static str,
        path: &str,
        query: &[(&str, &str)],
        range: Option<&str>,
    ) -> Result<StreamResponse, NightingaleError> {
        let mut req = self.authed_get(path, query);
        if let Some(r) = range {
            req = req.header("Range", r);
        }
        let resp = req
            .call()
            .map_err(|e| NightingaleError::jellyfin(stage, e))?;

        let status = resp.status().as_u16();
        let pick = |h: &str| -> Option<String> {
            resp.headers()
                .get(h)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string())
        };
        let content_type = pick("content-type");
        let content_range = pick("content-range");
        let accept_ranges = pick("accept-ranges");
        let content_length = pick("content-length").and_then(|s| s.parse::<u64>().ok());

        Ok(StreamResponse {
            status,
            content_type,
            content_range,
            accept_ranges,
            content_length,
            body: Box::new(StreamBody { resp }),
        })
    }
}

struct StreamBody {
    resp: ureq::http::Response<ureq::Body>,
}

impl Read for StreamBody {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.resp.body_mut().as_reader().read(buf)
    }
}

pub(crate) fn trim_base_url(url: &str) -> String {
    url.trim_end_matches('/').to_string()
}
