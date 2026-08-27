//! TheAudioDB (https://www.theaudiodb.com) integration -- currently just one
//! lookup: does an official YouTube music video exist for a song. Uses the
//! free v1 API with the documented shared test key ("123"); v2 is
//! premium-only (see https://www.theaudiodb.com/free_music_api's "Premium
//! also allows you to use the more modern V2 API"), so there's no per-user
//! API key to configure here. Free tier is capped at 30 requests/min per
//! TheAudioDB's docs -- fine for an on-demand, one-song-at-a-time lookup,
//! not for bulk-scanning a whole library at once.

use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use ts_rs::TS;

use crate::library_db;
use crate::song::Song;

const AUDIODB_API_KEY: &str = "123";

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

pub fn find_music_video_for_hash(file_hash: &str) -> Option<MusicVideoResult> {
    let song = library_db::load_song_by_hash(file_hash).ok().flatten()?;
    find_music_video(&song)
}
