use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use app_core::AppConfig;
use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{info, warn};

use crate::commands::ApiError;
use crate::events::EventBus;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct CastQuery {
    /// Free-text query for the fuzzy matcher. Ignored when `file_hash` is
    /// present.
    #[serde(default)]
    q: Option<String>,
    /// Direct, unambiguous song lookup -- what the "cast this instead"
    /// alternative links (offered when a fuzzy match turns out not to be
    /// analyzed) point at, so re-clicking one can't land on a *different*
    /// song the way re-running the fuzzy text search theoretically could.
    #[serde(default)]
    file_hash: Option<String>,
    /// 0.0-1.0 guide-vocal mix level, only meaningful for the custom
    /// receiver path (`ChromecastConfig.receiver_app_id`) -- ignored by the
    /// DefaultMediaReceiver path, which has no live audio mixing to
    /// control. Omitted -> receiver falls back to its own config default.
    #[serde(default)]
    guide_volume: Option<f64>,
}

struct CastOutcome {
    file_hash: String,
    title: String,
    artist: String,
}

#[derive(Serialize, Clone)]
struct AlternativeSong {
    file_hash: String,
    title: String,
    artist: String,
}

/// Distinct from a plain `ApiError` because "found a match but it isn't
/// analyzed yet" needs to carry structured data (the alternatives list) the
/// JSON path can return and the HTML path's WS event can render as actual
/// links -- a bare status+string `ApiError` has nowhere to put that.
enum CastError {
    Api(ApiError),
    NotAnalyzed {
        message: String,
        alternatives: Vec<AlternativeSong>,
    },
}

impl From<ApiError> for CastError {
    fn from(e: ApiError) -> Self {
        CastError::Api(e)
    }
}

impl IntoResponse for CastError {
    fn into_response(self) -> Response {
        match self {
            CastError::Api(e) => e.into_response(),
            CastError::NotAnalyzed { message, alternatives } => (
                StatusCode::CONFLICT,
                Json(json!({ "error": message, "alternatives": alternatives })),
            )
                .into_response(),
        }
    }
}

/// `GET /api/cast?q=<free text>` -- finds the best-matching local song for
/// `q` and casts it to the Chromecast configured in `config.json`. Cast-only:
/// does not touch `JukeboxState`, so it's independent of whichever browser
/// currently holds the WS controller role.
///
/// `Accept: application/json` gets the original synchronous contract
/// unchanged (blocks until done, returns JSON or the existing `ApiError`
/// status/body) -- this session's own curl-based testing relies on it.
/// Anything else (i.e. a browser) gets an HTML status page immediately;
/// the real work moves to a background task that reports progress over
/// the existing `/ws` event bus (`"cast-progress"` events) instead of
/// leaving the tab hanging blank for up to a minute on a first-time
/// karaoke-video render.
pub async fn handle_cast(
    State(state): State<AppState>,
    Query(query): Query<CastQuery>,
    headers: HeaderMap,
) -> Response {
    if query.q.is_none() && query.file_hash.is_none() {
        return ApiError(StatusCode::BAD_REQUEST, "must provide either q or file_hash".into())
            .into_response();
    }

    let wants_json = headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.contains("application/json"));

    if wants_json {
        return match run_cast(
            state.events.clone(),
            next_request_id(),
            query.q,
            query.file_hash,
            query.guide_volume,
        )
        .await
        {
            Ok(outcome) => Json(json!({
                "file_hash": outcome.file_hash,
                "title": outcome.title,
                "artist": outcome.artist,
            }))
            .into_response(),
            Err(e) => e.into_response(),
        };
    }

    let request_id = next_request_id();
    let display_query = match (&query.q, &query.file_hash) {
        (Some(q), _) => q.clone(),
        (None, Some(hash)) => format!("song {hash}"),
        (None, None) => String::new(),
    };
    let page = Html(render_status_page(&request_id, &display_query));
    tokio::spawn(run_cast(
        state.events.clone(),
        request_id,
        query.q,
        query.file_hash,
        query.guide_volume,
    ));
    page.into_response()
}

fn next_request_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{nanos:x}-{n:x}")
}

#[derive(Serialize)]
struct Progress<'a> {
    request_id: &'a str,
    stage: &'a str,
    message: String,
}

fn emit_progress(events: &EventBus, request_id: &str, stage: &str, message: impl Into<String>) {
    events.emit(
        "cast-progress",
        &Progress {
            request_id,
            stage,
            message: message.into(),
        },
    );
}

#[derive(Serialize)]
struct NotAnalyzedProgress<'a> {
    request_id: &'a str,
    stage: &'a str,
    message: String,
    alternatives: &'a [AlternativeSong],
}

/// Same `"cast-progress"` event stream as `emit_progress`, just with the
/// `alternatives` field the plain `Progress` shape has no room for -- the
/// status page's WS client keys off `stage: "not_analyzed"` to render these
/// as actual links instead of only showing the message.
fn emit_not_analyzed(
    events: &EventBus,
    request_id: &str,
    message: String,
    alternatives: &[AlternativeSong],
) {
    events.emit(
        "cast-progress",
        &NotAnalyzedProgress {
            request_id,
            stage: "not_analyzed",
            message,
            alternatives,
        },
    );
}

/// The actual match/render/cast sequence, shared by both the synchronous
/// JSON path (awaited directly) and the HTML path (spawned in the
/// background) -- they differ only in what happens with the result, not
/// in the logic itself.
async fn run_cast(
    events: Arc<EventBus>,
    request_id: String,
    query: Option<String>,
    file_hash: Option<String>,
    guide_volume: Option<f64>,
) -> Result<CastOutcome, CastError> {
    let query = query.unwrap_or_default();
    info!("[cast] query={:?} file_hash={:?}", query, file_hash);
    emit_progress(
        &events,
        &request_id,
        "matching",
        match file_hash.as_deref() {
            Some(hash) => format!("Looking up song {hash}..."),
            None => format!("Looking for a song matching {query:?}..."),
        },
    );

    let config = AppConfig::load();
    let Some(chromecast) = config.chromecast else {
        warn!("[cast] rejected: no chromecast configured in config.json");
        let msg = "no chromecast configured in config.json";
        emit_progress(&events, &request_id, "error", msg);
        return Err(ApiError(StatusCode::BAD_REQUEST, msg.into()).into());
    };

    // `file_hash` (an alternative-song link) is a direct, unambiguous
    // lookup; otherwise fall back to the fuzzy text matcher. Either way the
    // result still goes through the same not-analyzed check below --
    // clicking an alternative link can't skip it, it's just far less
    // likely to trip it since alternatives are pre-filtered to analyzed
    // songs.
    let song = match file_hash.as_deref() {
        Some(hash) => app_core::find_song_by_hash(hash),
        None => app_core::find_best_matching_local_song(&query),
    };
    let Some(song) = song else {
        let msg = match file_hash.as_deref() {
            Some(hash) => format!("no song found for file_hash {hash:?}"),
            None => format!("no confident song match for {query:?}"),
        };
        warn!("[cast] {msg}");
        emit_progress(&events, &request_id, "no_match", msg.clone());
        return Err(ApiError(StatusCode::NOT_FOUND, msg).into());
    };
    info!(
        "[cast] matched {:?} by {:?} (file_hash={}); handing off to chromecast module",
        song.title, song.artist, song.file_hash
    );

    // Casting (raw audio or, especially, karaoke video, which needs the
    // transcript to render lyrics) only ever makes sense for an analyzed
    // song -- rather than silently trying and hitting a render error deep
    // into the flow (or worse, quietly kicking off analysis as a side
    // effect the caller never asked for), fail fast here with alternatives
    // the user can click straight into instead.
    if !song.is_analyzed {
        warn!("[cast] matched {:?} but it isn't analyzed yet", song.title);
        let message = format!(
            "{:?} by {:?} hasn't been analyzed yet -- analyze it from the library before casting",
            song.title, song.artist
        );
        let alternatives: Vec<AlternativeSong> = app_core::find_alternative_analyzed_songs(&query, 5)
            .into_iter()
            .map(|s| AlternativeSong {
                file_hash: s.file_hash,
                title: s.title,
                artist: s.artist,
            })
            .collect();
        emit_not_analyzed(&events, &request_id, message.clone(), &alternatives);
        return Err(CastError::NotAnalyzed { message, alternatives });
    }

    emit_progress(
        &events,
        &request_id,
        "matched",
        format!("Found {:?} by {:?}", song.title, song.artist),
    );

    if chromecast.karaoke_video {
        emit_progress(
            &events,
            &request_id,
            "rendering",
            "Preparing video (rendering now if not already cached)...",
        );
    }
    emit_progress(&events, &request_id, "casting", "Casting to your Chromecast...");

    let cast_song = song.clone();
    let cast_result = tokio::task::spawn_blocking(move || {
        app_core::cast_song_to_configured_device(&chromecast, &cast_song, guide_volume)
    })
    .await
    .map_err(|e| {
        let msg = format!("cast task panicked: {e}");
        emit_progress(&events, &request_id, "error", msg.clone());
        ApiError(StatusCode::INTERNAL_SERVER_ERROR, msg)
    })?;

    if let Err(e) = cast_result {
        warn!("[cast] cast_song_to_configured_device failed: {e}");
        emit_progress(&events, &request_id, "error", e.to_string());
        return Err(ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into());
    }
    info!("[cast] cast succeeded for {:?}", song.title);
    emit_progress(
        &events,
        &request_id,
        "done",
        format!("Now playing {:?} by {:?}", song.title, song.artist),
    );

    Ok(CastOutcome {
        file_hash: song.file_hash,
        title: song.title,
        artist: song.artist,
    })
}

fn html_escape(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '&' => "&amp;".to_string(),
            '<' => "&lt;".to_string(),
            '>' => "&gt;".to_string(),
            '"' => "&quot;".to_string(),
            '\'' => "&#39;".to_string(),
            c => c.to_string(),
        })
        .collect()
}

fn render_status_page(request_id: &str, query: &str) -> String {
    let query_html = html_escape(query);
    let request_id_html = html_escape(request_id);
    format!(
        r##"<!doctype html>
<html>
<head>
<meta charset="utf-8">
<title>Casting…</title>
<style>
  body {{
    background: #0c0e18;
    color: #f2f2f5;
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100vh;
    margin: 0;
  }}
  .card {{
    text-align: center;
    max-width: 32rem;
    padding: 2rem;
  }}
  .query {{
    color: #9a9aa8;
    margin-bottom: 1.5rem;
  }}
  .spinner {{
    width: 2.5rem;
    height: 2.5rem;
    margin: 0 auto 1.5rem;
    border-radius: 50%;
    border: 3px solid #2a2d3d;
    border-top-color: #ffcd50;
    animation: spin 0.8s linear infinite;
  }}
  .icon {{
    font-size: 2.5rem;
    margin-bottom: 1.5rem;
  }}
  @keyframes spin {{ to {{ transform: rotate(360deg); }} }}
  #status {{ font-size: 1.15rem; }}
  .error #status {{ color: #ff7878; }}
  .done #status {{ color: #8cffa0; }}
  #alternatives {{ margin-top: 1.25rem; text-align: left; }}
  #alternatives:empty {{ display: none; }}
  #alternatives p {{ margin: 0 0 0.5rem; font-size: 0.85rem; color: #9a9aa8; }}
  #alternatives a {{
    display: block;
    padding: 0.5rem 0.75rem;
    margin-bottom: 0.4rem;
    border-radius: 0.5rem;
    background: #161925;
    color: #ffcd50;
    text-decoration: none;
  }}
  #alternatives a:hover {{ background: #1e2233; }}
</style>
</head>
<body>
  <div class="card" id="card">
    <div class="spinner" id="spinner"></div>
    <div class="icon" id="icon" style="display:none"></div>
    <div class="query">Searching for &ldquo;{query_html}&rdquo;</div>
    <div id="status">Starting…</div>
    <div id="alternatives"></div>
  </div>
  <script>
    const requestId = "{request_id_html}";
    const statusEl = document.getElementById("status");
    const cardEl = document.getElementById("card");
    const spinnerEl = document.getElementById("spinner");
    const iconEl = document.getElementById("icon");
    const alternativesEl = document.getElementById("alternatives");

    function setTerminal(cls, icon) {{
      cardEl.className = cls;
      spinnerEl.style.display = "none";
      iconEl.style.display = "block";
      iconEl.textContent = icon;
    }}

    // Built with DOM APIs (not innerHTML) so title/artist text -- which
    // arrives as plain JSON over the WS, not server-rendered -- can never
    // be interpreted as markup regardless of content.
    function showAlternatives(alternatives) {{
      alternativesEl.textContent = "";
      if (!alternatives || alternatives.length === 0) return;

      const heading = document.createElement("p");
      heading.textContent = "Cast one of these instead:";
      alternativesEl.appendChild(heading);

      for (const alt of alternatives) {{
        const link = document.createElement("a");
        link.href = `/api/cast?file_hash=${{encodeURIComponent(alt.file_hash)}}`;
        link.textContent = `${{alt.title}} — ${{alt.artist}}`;
        alternativesEl.appendChild(link);
      }}
    }}

    const ws = new WebSocket(`ws://${{location.host}}/ws`);
    ws.onmessage = (evt) => {{
      let msg;
      try {{ msg = JSON.parse(evt.data); }} catch {{ return; }}
      if (msg.type !== "cast-progress") return;
      const p = msg.payload || {{}};
      if (p.request_id !== requestId) return;

      statusEl.textContent = p.message || "";
      if (p.stage === "not_analyzed") {{
        setTerminal("error", "✕");
        showAlternatives(p.alternatives);
      }} else if (p.stage === "error" || p.stage === "no_match") {{
        setTerminal("error", "✕");
      }} else if (p.stage === "done") {{
        setTerminal("done", "✓");
      }}
    }};
    ws.onerror = () => {{
      statusEl.textContent = "Lost connection to the server.";
      setTerminal("error", "✕");
    }};
  </script>
</body>
</html>
"##
    )
}
