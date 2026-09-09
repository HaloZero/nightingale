//! Authenticated Plex Media Server HTTP client.
//!
//! Plex tokens are sent only in `X-Plex-Token`; they are never appended to a
//! URL. This matters for the local video proxy and for artwork/cache requests,
//! where URLs are commonly logged by HTTP infrastructure.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde::de::DeserializeOwned;
use ureq::Agent;

use crate::error::NightingaleError;
use crate::source::StreamResponse;

const PRODUCT: &str = "Nightingale";
const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone)]
pub struct PlexClient {
    agent: Agent,
    base_url: String,
    token: Arc<str>,
    client_id: Arc<str>,
}

impl PlexClient {
    pub fn new(
        base_url: impl Into<String>,
        token: impl Into<String>,
        client_id: impl Into<String>,
    ) -> Self {
        let config = Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(10)))
            .timeout_recv_response(Some(Duration::from_secs(30)))
            .build();
        Self {
            agent: Agent::new_with_config(config),
            base_url: trim_base_url(&base_url.into()),
            token: Arc::from(token.into()),
            client_id: Arc::from(client_id.into()),
        }
    }

    pub fn for_downloads(
        base_url: impl Into<String>,
        token: impl Into<String>,
        client_id: impl Into<String>,
    ) -> Self {
        let config = Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(10)))
            .build();
        Self {
            agent: Agent::new_with_config(config),
            base_url: trim_base_url(&base_url.into()),
            token: Arc::from(token.into()),
            client_id: Arc::from(client_id.into()),
        }
    }

    /// Short timeouts for probing Plex-advertised alternatives. Discovery may
    /// include stale LAN, WAN, and relay addresses; one dead address must not
    /// hold the sign-in flow for the normal request timeout.
    pub fn for_discovery(
        base_url: impl Into<String>,
        token: impl Into<String>,
        client_id: impl Into<String>,
    ) -> Self {
        let config = Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(3)))
            .timeout_recv_response(Some(Duration::from_secs(8)))
            .build();
        Self {
            agent: Agent::new_with_config(config),
            base_url: trim_base_url(&base_url.into()),
            token: Arc::from(token.into()),
            client_id: Arc::from(client_id.into()),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    fn endpoint(&self, path: &str) -> Result<String, NightingaleError> {
        // Server-provided metadata/part keys must remain server-relative. Do
        // not allow an absolute key to turn this authenticated client into a
        // credential-forwarding proxy for another host.
        if path.starts_with("http://")
            || path.starts_with("https://")
            || path.starts_with("//")
            || path_has_token(path)
        {
            return Err(NightingaleError::Other(
                "Plex returned an unsafe media path".into(),
            ));
        }
        Ok(format!(
            "{}/{}",
            self.base_url,
            path.trim_start_matches('/')
        ))
    }

    fn authed_get(
        &self,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<ureq::RequestBuilder<ureq::typestate::WithoutBody>, NightingaleError> {
        let mut req = self
            .agent
            .get(self.endpoint(path)?)
            .header("Accept", "application/json")
            .header("X-Plex-Token", self.token.as_ref())
            .header("X-Plex-Client-Identifier", self.client_id.as_ref())
            .header("X-Plex-Product", PRODUCT)
            .header("X-Plex-Version", VERSION);
        for (key, value) in query {
            req = req.query(*key, *value);
        }
        Ok(req)
    }

    pub fn get_json<T: DeserializeOwned>(
        &self,
        stage: &'static str,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<T, NightingaleError> {
        let response = self
            .authed_get(path, query)?
            .call()
            .map_err(|error| NightingaleError::plex(stage, error))?;
        response
            .into_body()
            .read_json()
            .map_err(|error| NightingaleError::plex(stage, error))
    }

    pub fn download_to_vec(
        &self,
        stage: &'static str,
        path: &str,
    ) -> Result<Vec<u8>, NightingaleError> {
        let response = self
            .authed_get(path, &[])?
            .call()
            .map_err(|error| NightingaleError::plex(stage, error))?;
        let mut body = response.into_body();
        let mut reader = body.as_reader();
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes)?;
        Ok(bytes)
    }

    pub fn download_to_file(
        &self,
        stage: &'static str,
        path: &str,
        dest: &Path,
    ) -> Result<PathBuf, NightingaleError> {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Tell PMS this is an original-file download, not an ad-hoc playback
        // request. Without this flag Plex may try to make a streaming decision
        // and return 5xx for otherwise downloadable audio.
        let response = self
            .authed_get(path, &[("download", "1")])?
            .call()
            .map_err(|error| NightingaleError::plex(stage, error))?;
        let temporary = dest.with_extension("part");
        {
            let mut body = response.into_body();
            let mut reader = body.as_reader();
            let mut file = std::fs::File::create(&temporary)?;
            std::io::copy(&mut reader, &mut file)?;
            file.flush()?;
        }
        std::fs::rename(&temporary, dest)?;
        Ok(dest.to_path_buf())
    }

    pub fn open_stream(
        &self,
        stage: &'static str,
        path: &str,
        range: Option<&str>,
    ) -> Result<StreamResponse, NightingaleError> {
        let mut request = self.authed_get(path, &[])?;
        if let Some(range) = range {
            request = request.header("Range", range);
        }
        let response = request
            .call()
            .map_err(|error| NightingaleError::plex(stage, error))?;

        let status = response.status().as_u16();
        let header = |name: &str| -> Option<String> {
            response
                .headers()
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned)
        };
        let content_type = header("content-type");
        let content_range = header("content-range");
        let accept_ranges = header("accept-ranges");
        let content_length = header("content-length").and_then(|value| value.parse().ok());

        Ok(StreamResponse {
            status,
            content_type,
            content_range,
            accept_ranges,
            content_length,
            body: Box::new(StreamBody { response }),
        })
    }
}

struct StreamBody {
    response: ureq::http::Response<ureq::Body>,
}

impl Read for StreamBody {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.response.body_mut().as_reader().read(buffer)
    }
}

fn path_has_token(path: &str) -> bool {
    let Ok(url) = url::Url::parse(&format!("http://nightingale.invalid/{path}")) else {
        return true;
    };
    url.query_pairs()
        .any(|(key, _)| key.eq_ignore_ascii_case("X-Plex-Token"))
}

pub(crate) fn trim_base_url(url: &str) -> String {
    url.trim_end_matches('/').to_owned()
}
