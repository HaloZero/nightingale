//! Thin Subsonic HTTP client.
//!
//! Subsonic auth is stateless per-request: every URL carries
//! `?u=<user>&t=MD5(password+salt)&s=<salt>&v=1.16.1&c=Nightingale&f=json`
//! (see <https://www.subsonic.org/pages/api.jsp>). We persist the password
//! in the secret-at-rest envelope and generate a fresh salt+token for each
//! call, so the on-the-wire token never reaches the disk.
//!
//! Every Subsonic response is wrapped:
//! ```json
//! { "subsonic-response": { "status": "ok|failed", "version": "...", ... } }
//! ```
//! `get_json` unwraps that envelope, surfaces `status == "failed"` via
//! `NightingaleError::Navidrome`, and otherwise hands the inner payload to
//! whoever asked.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use md5::{Digest, Md5};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use ureq::Agent;

use crate::error::NightingaleError;
use crate::source::StreamResponse;

const API_VERSION: &str = "1.16.1";
const CLIENT_NAME: &str = "Nightingale";

/// Credentials needed to derive the per-request auth params. Cloning is cheap
/// — both fields are `Arc<str>` — so each `SubsonicClient` keeps its own copy.
#[derive(Clone)]
pub struct AuthCreds {
    username: Arc<str>,
    password: Arc<str>,
}

impl AuthCreds {
    pub fn new(username: impl Into<String>, password: impl Into<String>) -> Self {
        Self {
            username: Arc::from(username.into()),
            password: Arc::from(password.into()),
        }
    }

    pub fn username(&self) -> &str {
        &self.username
    }

    /// Fresh `(salt, token)` for one request. Salt is 16 random hex chars
    /// per the Subsonic recommendation; token is the lowercase MD5 hex of
    /// `password || salt`.
    fn salt_and_token(&self) -> (String, String) {
        let salt_bytes: [u8; 8] = rand::random();
        let salt = hex_lower(&salt_bytes);
        let mut hasher = Md5::new();
        hasher.update(self.password.as_bytes());
        hasher.update(salt.as_bytes());
        let token = hex_lower(&hasher.finalize());
        (salt, token)
    }
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

#[derive(Clone)]
pub struct SubsonicClient {
    agent: Agent,
    base_url: String,
    creds: AuthCreds,
}

impl SubsonicClient {
    pub fn new(base_url: impl Into<String>, creds: AuthCreds) -> Self {
        let config = Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(10)))
            .timeout_recv_response(Some(Duration::from_secs(30)))
            .build();
        let agent = Agent::new_with_config(config);
        Self {
            agent,
            base_url: trim_base_url(&base_url.into()),
            creds,
        }
    }

    /// Variant without a read-response timeout — used for streaming
    /// downloads where the request itself may take arbitrarily long but
    /// bytes arrive continuously.
    pub fn for_downloads(base_url: impl Into<String>, creds: AuthCreds) -> Self {
        let config = Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(10)))
            .build();
        let agent = Agent::new_with_config(config);
        Self {
            agent,
            base_url: trim_base_url(&base_url.into()),
            creds,
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn username(&self) -> &str {
        self.creds.username()
    }

    fn endpoint(&self, path: &str) -> String {
        let trimmed = path.trim_start_matches('/');
        format!("{}/{}", self.base_url, trimmed)
    }

    /// Build a `GET` request with the standard Subsonic auth + format params
    /// applied. Caller appends any endpoint-specific query string entries.
    fn authed_get(
        &self,
        path: &str,
        query: &[(&str, &str)],
    ) -> ureq::RequestBuilder<ureq::typestate::WithoutBody> {
        let (salt, token) = self.creds.salt_and_token();
        let mut req = self
            .agent
            .get(self.endpoint(path))
            .header("Accept", "application/json")
            .query("u", self.creds.username())
            .query("t", &token)
            .query("s", &salt)
            .query("v", API_VERSION)
            .query("c", CLIENT_NAME)
            .query("f", "json");
        for (k, v) in query {
            req = req.query(*k, *v);
        }
        req
    }

    /// Call a JSON endpoint and unwrap the `subsonic-response` envelope.
    /// Server-reported failures (`status: "failed"`) surface as
    /// `NightingaleError::Navidrome`; transport / parse errors do the same.
    pub fn get_json<T: DeserializeOwned>(
        &self,
        stage: &'static str,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<T, NightingaleError> {
        let resp = self
            .authed_get(path, query)
            .call()
            .map_err(|e| NightingaleError::navidrome(stage, e))?;
        let envelope: Envelope<T> = resp
            .into_body()
            .read_json()
            .map_err(|e| NightingaleError::navidrome(stage, e))?;
        envelope.unwrap(stage)
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
            .map_err(|e| NightingaleError::navidrome(stage, e))?;
        let mut body = resp.into_body();
        let mut reader = body.as_reader();
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes)?;
        Ok(bytes)
    }

    /// Stream a download to disk. Writes to `<dest>.part` first and renames
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
            .map_err(|e| NightingaleError::navidrome(stage, e))?;
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

    /// Open a streaming response. The Navidrome source itself doesn't proxy
    /// streams today (audio-only library — playback always goes through the
    /// materialised cache file), but the helper is here for symmetry with
    /// Jellyfin in case we ever want a remote-stream route.
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
            .map_err(|e| NightingaleError::navidrome(stage, e))?;

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

// ─── Envelope ────────────────────────────────────────────────────────

/// Subsonic always wraps responses as `{"subsonic-response": {...}}`. The
/// inner block carries a `status` field (`"ok"` or `"failed"`) and, on
/// failure, an `error` block with a `code` + `message`. `T` is whatever
/// payload the endpoint adds (`album`, `albumList2`, …).
#[derive(Debug, Deserialize)]
struct Envelope<T> {
    #[serde(rename = "subsonic-response")]
    response: Inner<T>,
}

#[derive(Debug, Deserialize)]
struct Inner<T> {
    #[serde(default)]
    status: String,
    #[serde(default)]
    error: Option<SubsonicError>,
    #[serde(flatten)]
    payload: T,
}

#[derive(Debug, Deserialize)]
struct SubsonicError {
    #[serde(default)]
    code: i32,
    #[serde(default)]
    message: String,
}

impl std::fmt::Display for SubsonicError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.message.is_empty() {
            write!(f, "subsonic error code {}", self.code)
        } else {
            write!(f, "[{}] {}", self.code, self.message)
        }
    }
}

impl std::error::Error for SubsonicError {}

impl<T> Envelope<T> {
    fn unwrap(self, stage: &'static str) -> Result<T, NightingaleError> {
        if self.response.status == "ok" {
            return Ok(self.response.payload);
        }
        let err = self.response.error.unwrap_or(SubsonicError {
            code: 0,
            message: "missing error block".into(),
        });
        Err(NightingaleError::navidrome(stage, err))
    }
}
