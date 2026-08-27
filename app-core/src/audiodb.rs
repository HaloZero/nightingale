//! TheAudioDB (https://www.theaudiodb.com) integration -- currently just one
//! lookup: does an official YouTube music video exist for a song. Uses the
//! free v1 API with the documented shared test key ("123"); v2 is
//! premium-only (see https://www.theaudiodb.com/free_music_api's "Premium
//! also allows you to use the more modern V2 API"), so there's no per-user
//! API key to configure here. Free tier is capped at 30 requests/min per
//! TheAudioDB's docs -- `find_music_video`'s own throttle (see
//! `MIN_LOOKUP_INTERVAL`) keeps every caller under that, including a bulk
//! action over a whole filtered library, just slowly for a cold cache.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use ts_rs::TS;

use crate::library_db;
use crate::song::Song;

const AUDIODB_API_KEY: &str = "123";

/// TheAudioDB's free tier is 30 requests/minute (2s/request average); 2.5s
/// leaves margin. A single on-demand lookup never notices this, but a bulk
/// action (e.g. "Fetch YouTube karaoke video" over a whole filtered library)
/// would otherwise fire a lookup per song back-to-back and blow through the
/// limit on the first couple hundred songs of a cold cache. Lives here
/// (not just in the bulk path) so every caller is protected uniformly, cache
/// hits included -- `find_music_video_for_hash`'s cache check happens
/// before this, so a cached result never waits.
const MIN_LOOKUP_INTERVAL: Duration = Duration::from_millis(2_500);
static LAST_LOOKUP_START: Mutex<Option<Instant>> = Mutex::new(None);

fn throttle() {
    let mut last = LAST_LOOKUP_START.lock().unwrap();
    if let Some(prev) = *last {
        let elapsed = prev.elapsed();
        if elapsed < MIN_LOOKUP_INTERVAL {
            let wait = MIN_LOOKUP_INTERVAL - elapsed;
            info!(
                "[audiodb] throttling: waiting {:.1}s before next lookup (free tier is 30/min)",
                wait.as_secs_f64()
            );
            std::thread::sleep(wait);
        }
    }
    *last = Some(Instant::now());
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct MusicVideoResult {
    pub youtube_url: String,
    pub track_name: String,
    pub artist_name: String,
}

#[derive(Debug, Deserialize)]
struct SearchTrackResponse {
    track: Option<Vec<AudioDbTrack>>,
}

#[derive(Debug, Deserialize)]
struct AudioDbTrack {
    #[serde(rename = "strTrack")]
    str_track: String,
    #[serde(rename = "strArtist")]
    str_artist: String,
    // `null` both when TheAudioDB has the track but no video on file, and
    // implicitly via a missing `track` array entirely when nothing matched
    // the search at all -- both cases just fall out of this being `None`.
    #[serde(rename = "strMusicVid")]
    str_music_vid: Option<String>,
}

/// Looks up `song` on TheAudioDB by artist+title and returns its official
/// YouTube music video URL, if TheAudioDB has one on file. `None` covers
/// both "no track match" and "track found but no music video" -- callers
/// don't need to tell those apart.
pub fn find_music_video(song: &Song) -> Option<MusicVideoResult> {
    if song.title.is_empty() || song.artist.is_empty() || song.artist == "Unknown Artist" {
        return None;
    }

    throttle();

    info!(
        "[audiodb] searching: \"{}\" by \"{}\"",
        song.title, song.artist
    );

    let url = format!(
        "https://www.theaudiodb.com/api/v1/json/{AUDIODB_API_KEY}/searchtrack.php?s={}&t={}",
        urlencoding::encode(&song.artist),
        urlencoding::encode(&song.title),
    );

    let resp = match ureq::get(&url)
        .header("User-Agent", "Nightingale/1.0")
        .call()
    {
        Ok(r) => r,
        Err(e) => {
            warn!("[audiodb] search request failed: {e}");
            return None;
        }
    };

    let parsed: SearchTrackResponse = match resp.into_body().read_json() {
        Ok(r) => r,
        Err(e) => {
            warn!("[audiodb] failed to parse search response: {e}");
            return None;
        }
    };

    let result = parsed.track.into_iter().flatten().find_map(|t| {
        let youtube_url = t.str_music_vid.filter(|v| !v.trim().is_empty())?;
        Some(MusicVideoResult {
            youtube_url,
            track_name: t.str_track,
            artist_name: t.str_artist,
        })
    });

    match &result {
        Some(r) => info!(
            "[audiodb] found music video for \"{}\": {}",
            song.title, r.youtube_url
        ),
        None => info!(
            "[audiodb] no music video found for \"{}\" by \"{}\"",
            song.title, song.artist
        ),
    }

    result
}

/// Same as `find_music_video`, but checks `library_db`'s
/// `youtube_video_lookups` cache first and only calls TheAudioDB on a
/// genuine cache miss (no row for this hash yet) -- repeat calls for the
/// same song (e.g. re-casting, re-rendering) read the cached outcome
/// instead of spending against the free tier's 30-requests/minute limit.
/// A cached "looked up, nothing found" result is honored too, not just a
/// cached hit -- otherwise a song with no video would get re-queried every
/// single time.
pub fn find_music_video_for_hash(file_hash: &str) -> Option<MusicVideoResult> {
    match library_db::get_youtube_video_lookup(file_hash) {
        Ok(Some(cached)) => {
            info!("[audiodb] using cached lookup for {file_hash}, not calling the API");
            return cached.youtube_url.map(|youtube_url| MusicVideoResult {
                youtube_url,
                track_name: cached.track_name.unwrap_or_default(),
                artist_name: cached.artist_name.unwrap_or_default(),
            });
        }
        Ok(None) => {}
        Err(e) => warn!("[audiodb] failed to read cached lookup for {file_hash}: {e}"),
    }

    let song = library_db::load_song_by_hash(file_hash).ok().flatten()?;
    let result = find_music_video(&song);

    if let Err(e) = library_db::record_youtube_video_lookup(
        file_hash,
        result.as_ref().map(|r| r.youtube_url.as_str()),
        result.as_ref().map(|r| r.track_name.as_str()),
        result.as_ref().map(|r| r.artist_name.as_str()),
    ) {
        warn!("[audiodb] failed to cache lookup result for {file_hash}: {e}");
    }

    result
}
