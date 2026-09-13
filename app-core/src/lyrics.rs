use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};
use ts_rs::TS;

use crate::analyzer::{
    enqueue_one, is_usdx_song, mark_stems_only, prepare_lrc_no_stems, update_song_analyzed,
};
use crate::cache::CacheDir;
use crate::library_db;
use crate::lrc::{self, ParsedLrc};
use crate::song::{Song, TranscriptSource, read_transcript_meta};

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LrclibCandidate {
    #[serde(default, alias = "trackName")]
    pub track_name: String,
    #[serde(default, alias = "artistName")]
    pub artist_name: String,
    #[serde(default, alias = "albumName")]
    pub album_name: String,
    #[serde(default, alias = "duration")]
    pub duration_secs: f64,
    #[serde(skip_deserializing, default)]
    pub lines: Vec<String>,
    /// Raw LRC (line-level synced lyrics) from LRCLIB, when available. Exposed
    /// to the frontend so the editor can offer timed lyrics without alignment.
    /// `alias` (not `rename`) so it deserializes from LRCLIB's `syncedLyrics`
    /// but still serializes as `synced_lyrics` for the frontend type.
    #[serde(default, alias = "syncedLyrics")]
    pub synced_lyrics: Option<String>,
    #[serde(default, rename = "plainLyrics", skip_serializing)]
    #[ts(skip)]
    plain_lyrics: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LyricsFile {
    pub lines: Vec<String>,
    /// Best-effort per-line timing anchors sourced from LRCLIB's
    /// `syncedLyrics`, fuzzy-matched against `lines` in the same order (see
    /// `match_synced_anchors`). Same length as `lines` when present; `None`
    /// entries mark lines with no confident match. Absent entirely when no
    /// LRCLIB synced source was found/available.
    ///
    /// Consumed only by the Python analyzer (`align.py`) as alignment-window
    /// hints to shrink each forced-alignment segment from "the whole song"
    /// to "around this one line" -- never as the displayed lyric text or
    /// wording, which remains `lines`, sourced independently and possibly
    /// from a different provider than these anchors.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_anchors: Option<Vec<Option<f64>>>,
}

fn normalize(s: &str) -> String {
    s.trim().to_lowercase()
}

/// Minimum `jaro_winkler` similarity (same normalized-lowercase, trimmed
/// comparison `search.rs` uses) between a candidate's LRCLIB album name and
/// the song's own tagged album to count as "the same release" in
/// `lrclib_candidates`'s ranking. Calibrated against a real mismatch case
/// (Britney Spears' "Toxic"): genuine spelling/punctuation variants of the
/// same album ("Greatest Hits: My Prerogative" vs "Greatest Hits - My
/// Prerogative - CD1" vs the full-width-colon "Greatest Hits：My
/// Prerogative") scored 0.88-0.99 against each other, while unrelated
/// albums LRCLIB returned for the same song ("Woman", "Drivetime
/// Anthems", "The Singles Collection") scored 0.32-0.58 -- 0.8 sits
/// cleanly in the gap between those two clusters.
const ALBUM_MATCH_THRESHOLD: f64 = 0.8;

/// Minimum `jaro_winkler` similarity between an LRCLIB synced-line's text and
/// a display line to accept it as that line's timing anchor in
/// `match_synced_anchors`. Looser than `ALBUM_MATCH_THRESHOLD` because line
/// wording drifts more between sources than album naming (e.g. this song's
/// own "Everytime" vs "Every time we're down").
const LINE_MATCH_THRESHOLD: f64 = 0.75;

/// How many upcoming LRCLIB synced segments `match_synced_anchors` scans for
/// each display line. Mirrors `align.py`'s `_map_words_to_lines`
/// `LOOKAHEAD = 6`, widened slightly because a line-level source can
/// insert/omit whole lines (ad-libs, repeated hooks) more often than a
/// single word gets dropped mid-line.
const ANCHOR_LOOKAHEAD: usize = 8;

/// Below this fraction of matched lines, treat the whole anchor set as noise
/// (most likely a wrong LRCLIB pick that only coincidentally shares a couple
/// of common lines) rather than a genuine partial match, and attach no
/// anchors at all -- degrades to today's whole-song alignment behavior
/// instead of risking a few confidently-wrong anchors corrupting alignment
/// across large stretches of the song.
const MIN_ANCHOR_COVERAGE: f64 = 0.25;

/// Fuzzy-match `display_lines` (the lyric text already chosen for display,
/// from whichever source won -- local file, embedded tag, or LRCLIB plain
/// text) against `parsed`'s LRCLIB-derived synced segments, in order, to
/// produce one optional timing anchor (that segment's start, in seconds) per
/// display line.
///
/// Sequential two-pointer walk, best-of-window rather than first-hit: for
/// each display line, scan up to `ANCHOR_LOOKAHEAD` synced segments starting
/// at the cursor, keep the highest-scoring match at or above
/// `LINE_MATCH_THRESHOLD`, and advance the cursor just past it. The cursor
/// never advances on a miss, so a display line with no match doesn't block a
/// later line from matching further down the synced stream. `parsed.segments`
/// is already start-time sorted (`lrc::parse_lrc` sorts its entries), and the
/// cursor only moves forward, so the returned anchors come out non-decreasing
/// in order automatically -- no extra sort/clamp needed downstream.
fn match_synced_anchors(display_lines: &[String], parsed: &ParsedLrc) -> Vec<Option<f64>> {
    let mut cursor = 0usize;
    let mut anchors = Vec::with_capacity(display_lines.len());
    for line in display_lines {
        let norm_line = normalize(line);
        let limit = (cursor + ANCHOR_LOOKAHEAD).min(parsed.segments.len());
        let mut best: Option<(usize, f64)> = None;
        for idx in cursor..limit {
            let score = strsim::jaro_winkler(&norm_line, &normalize(&parsed.segments[idx].text));
            if score >= LINE_MATCH_THRESHOLD && best.is_none_or(|(_, b)| score > b) {
                best = Some((idx, score));
            }
        }
        match best {
            Some((idx, _)) => {
                anchors.push(Some(parsed.segments[idx].start));
                cursor = idx + 1;
            }
            None => anchors.push(None),
        }
    }
    anchors
}

/// `None` if `anchors` is empty or has too few matched lines
/// (`MIN_ANCHOR_COVERAGE`) to trust, otherwise `Some(anchors)` unchanged.
fn anchors_if_sufficient(anchors: Vec<Option<f64>>) -> Option<Vec<Option<f64>>> {
    if anchors.is_empty() {
        return None;
    }
    let matched = anchors.iter().filter(|a| a.is_some()).count();
    if (matched as f64 / anchors.len() as f64) < MIN_ANCHOR_COVERAGE {
        return None;
    }
    Some(anchors)
}

pub(crate) fn lrclib_candidates(song: &Song) -> Vec<LrclibCandidate> {
    let title = &song.title;
    let artist = &song.artist;

    if title.is_empty() || artist == "Unknown Artist" {
        return Vec::new();
    }

    let agent = ureq::Agent::new_with_defaults();

    info!(
        "[lrclib] Searching: \"{title}\" by \"{artist}\" ({:.0}s, album=\"{}\")",
        song.duration_secs, song.album
    );

    // LRCLIB's search endpoint accepts an `album_name` filter that narrows
    // results server-side -- confirmed a real filter, not just a ranking
    // hint: an album that doesn't match anything in its catalog returns
    // zero results rather than falling back to a broader match. Try it
    // first: a hit means every candidate actually belongs to the right
    // release, rather than being picked from whatever LRCLIB's default
    // title+artist search happens to return (capped at 20, ranked by its
    // own relevance/popularity -- not guaranteed to even include the
    // release this file is actually from). Falls back to the plain
    // title+artist search if the narrowed one comes back empty, so a
    // tagged album that just doesn't match LRCLIB's naming isn't a dead
    // end.
    let results = if song.album.is_empty() || song.album == "Unknown Album" {
        lrclib_search(&agent, title, artist, None)
    } else {
        let narrowed = lrclib_search(&agent, title, artist, Some(&song.album));
        if narrowed.is_empty() {
            info!(
                "[lrclib] No results narrowed to album \"{}\", retrying without it",
                song.album
            );
            lrclib_search(&agent, title, artist, None)
        } else {
            info!(
                "[lrclib] {} result(s) narrowed to album \"{}\"",
                narrowed.len(),
                song.album
            );
            narrowed
        }
    };

    let mut with_lyrics: Vec<_> = results
        .into_iter()
        .filter(|r| {
            !r.plain_lyrics.is_empty()
                || r.synced_lyrics
                    .as_deref()
                    .is_some_and(|s| !s.trim().is_empty())
        })
        .collect();

    info!(
        "[lrclib] Search returned {} results with lyrics",
        with_lyrics.len()
    );

    // Fuzzy rather than exact album match: LRCLIB's own album strings vary
    // too much release to release for an exact-lowercase-equality check to
    // land often -- e.g. "Greatest Hits: My Prerogative" vs "Greatest Hits
    // - My Prerogative - CD1" vs "Greatest Hits：My Prerogative" (full-width
    // colon) all clearly name the same release but never compare equal.
    // `jaro_winkler` (same fuzzy-match approach `search.rs` uses for
    // free-text song lookup) scores those variants 0.88-0.99 against each
    // other, while genuinely unrelated albums score well under 0.6 -- see
    // `ALBUM_MATCH_THRESHOLD`'s doc comment for real numbers. Below the
    // threshold, still no bonus at all (not a sliding scale) so a
    // near-but-not-quite match doesn't get to outweigh a real duration
    // difference.
    let album_norm = normalize(&song.album);
    with_lyrics.sort_by_key(|r| {
        let album_similarity = strsim::jaro_winkler(&album_norm, &normalize(&r.album_name));
        let album_bonus: i64 = if album_similarity >= ALBUM_MATCH_THRESHOLD {
            0
        } else {
            5_000
        };
        let duration_penalty = ((r.duration_secs - song.duration_secs).abs() * 10.0) as i64;
        album_bonus + duration_penalty
    });

    with_lyrics
        .into_iter()
        .filter_map(|mut r| {
            r.lines = r
                .plain_lyrics
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect();
            // Normalize empty synced payloads to `None` so the frontend can
            // treat "has LRC" as a simple presence check.
            if r.synced_lyrics
                .as_deref()
                .is_some_and(|s| s.trim().is_empty())
            {
                r.synced_lyrics = None;
            }
            if r.lines.is_empty() && r.synced_lyrics.is_none() {
                None
            } else {
                Some(r)
            }
        })
        .collect()
}

fn lrclib_search(
    agent: &ureq::Agent,
    title: &str,
    artist: &str,
    album: Option<&str>,
) -> Vec<LrclibCandidate> {
    let mut url = format!(
        "https://lrclib.net/api/search?track_name={}&artist_name={}",
        urlencoding::encode(title),
        urlencoding::encode(artist),
    );
    if let Some(album) = album {
        url.push_str("&album_name=");
        url.push_str(&urlencoding::encode(album));
    }

    let resp = match agent
        .get(&url)
        .header("User-Agent", "Nightingale/1.0")
        .call()
    {
        Ok(r) => r,
        Err(e) => {
            warn!("[lrclib] Search request failed: {e}");
            return Vec::new();
        }
    };
    match resp.into_body().read_json() {
        Ok(r) => r,
        Err(e) => {
            warn!("[lrclib] Failed to parse search results: {e}");
            Vec::new()
        }
    }
}

pub fn search_lrclib_for_hash(file_hash: &str) -> Vec<LrclibCandidate> {
    let Some(song) = library_db::load_song_by_hash(file_hash).ok().flatten() else {
        return Vec::new();
    };
    lrclib_candidates(&song)
}

pub fn load_lyrics_file(file_hash: &str) -> Option<LyricsFile> {
    let cache = CacheDir::new();
    let path = cache.lyrics_path(file_hash);
    if !path.is_file() {
        return None;
    }
    let bytes = std::fs::read(&path).ok()?;
    serde_json::from_slice::<LyricsFile>(&bytes).ok()
}

pub fn save_lyrics_and_realign(file_hash: &str, lines: Vec<String>) -> Result<(), String> {
    if is_usdx_song(file_hash) {
        return Err("Cannot edit lyrics for USDX songs".to_string());
    }

    let normalized: Vec<String> = lines
        .into_iter()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();

    if normalized.is_empty() {
        return Err("Lyrics cannot be empty".to_string());
    }

    let cache = CacheDir::new();
    let previous_language = library_db::load_song_by_hash(file_hash)
        .ok()
        .flatten()
        .and_then(|song| song.language);
    // `None`: the user hand-edited these lyrics, so any prior LRCLIB timing
    // anchors no longer correspond to this text and must not carry forward.
    write_lyrics_file(&cache, file_hash, &normalized, None)
        .map_err(|e| format!("Failed to write lyrics file: {e}"))?;

    let _ = std::fs::remove_file(cache.transcript_path(file_hash));
    cache.delete_transcript_variants(file_hash);

    update_song_analyzed(file_hash, false, previous_language, None, None, None);
    enqueue_one(file_hash);
    Ok(())
}

/// Build the transcript JSON (playback shape) from parsed LRC segments.
fn build_lrc_transcript(
    parsed: &ParsedLrc,
    language: Option<&str>,
    key: Option<&str>,
    tempo: f64,
    no_stems: bool,
) -> serde_json::Value {
    serde_json::json!({
        // Leave language null when unknown so it isn't later mistaken for a
        // forced alignment language override (whisperx has no "unknown" model).
        "language": language,
        "source": "lrc",
        "key": key,
        "tempo": tempo,
        "no_stems": no_stems,
        "segments": parsed.segments,
    })
}

fn write_transcript_json(
    cache: &CacheDir,
    file_hash: &str,
    value: &serde_json::Value,
) -> std::io::Result<()> {
    let out = cache.transcript_path(file_hash);
    let json = serde_json::to_vec_pretty(value).map_err(std::io::Error::other)?;
    std::fs::write(&out, json)
}

/// Provide LRC / Enhanced LRC for a not-yet-analyzed song, building the
/// transcript directly and skipping transcription. When `separate_stems` is
/// true, a stems-only analysis pass is queued (guide vocals + karaoke
/// instrumental); otherwise the song plays over its original mix and the guide
/// control is hidden on playback.
pub fn provide_lrc(file_hash: &str, lrc_text: &str, separate_stems: bool) -> Result<(), String> {
    if is_usdx_song(file_hash) {
        return Err("Cannot provide lyrics for USDX songs".to_string());
    }

    let parsed = lrc::parse_lrc(lrc_text)?;

    let Some(song) = library_db::load_song_by_hash(file_hash).ok().flatten() else {
        return Err("Song not found".to_string());
    };

    let cache = CacheDir::new();
    cache.delete_transcript_variants(file_hash);
    let _ = std::fs::remove_file(cache.lyrics_path(file_hash));

    let language = song.language.clone();

    if separate_stems {
        let value = build_lrc_transcript(&parsed, language.as_deref(), None, 1.0, false);
        write_transcript_json(&cache, file_hash, &value)
            .map_err(|e| format!("Failed to write transcript: {e}"))?;
        // Stays not-analyzed until stem separation finishes.
        update_song_analyzed(file_hash, false, language, None, None, None);
        mark_stems_only(file_hash);
        enqueue_one(file_hash);
    } else {
        let value = build_lrc_transcript(&parsed, language.as_deref(), None, 1.0, true);
        write_transcript_json(&cache, file_hash, &value)
            .map_err(|e| format!("Failed to write transcript: {e}"))?;
        // Playing over the original mix needs no separation. Prepare everything
        // synchronously (materialize audio, detect the key) and only then mark
        // the song ready — no status-queue pass, and no transient window where
        // playback assets aren't in place yet.
        prepare_lrc_no_stems(file_hash).map_err(|e| e.to_string())?;
    }

    Ok(())
}

/// Apply provided timed LRC to an already-analyzed song, rebuilding the
/// transcript directly (no realignment) while keeping the existing stems.
pub fn apply_timed_lyrics(file_hash: &str, lrc_text: &str) -> Result<(), String> {
    if is_usdx_song(file_hash) {
        return Err("Cannot edit lyrics for USDX songs".to_string());
    }

    let parsed = lrc::parse_lrc(lrc_text)?;

    let Some(song) = library_db::load_song_by_hash(file_hash).ok().flatten() else {
        return Err("Song not found".to_string());
    };

    let cache = CacheDir::new();
    let meta = read_transcript_meta(&cache, file_hash);
    // Base (unshifted) key so timings line up with the canonical stems.
    let key = song.key.clone().or(meta.key);
    let no_stems = song.no_stems;

    // Timing changed: drop any tempo-shifted transcript variants and the plain
    // lyrics sidecar, and reset the song back to its base key/tempo.
    cache.delete_transcript_variants(file_hash);
    let _ = std::fs::remove_file(cache.lyrics_path(file_hash));

    let value = build_lrc_transcript(
        &parsed,
        song.language.as_deref(),
        key.as_deref(),
        1.0,
        no_stems,
    );
    write_transcript_json(&cache, file_hash, &value)
        .map_err(|e| format!("Failed to write transcript: {e}"))?;

    let mut updated = song;
    updated.is_analyzed = true;
    updated.transcript_source = Some(TranscriptSource::Lrc);
    updated.key = key;
    updated.override_key = None;
    updated.tempo = 1.0;
    updated.key_offset = 0;
    updated.no_stems = no_stems;
    library_db::update_song_fields(file_hash, &updated).map_err(|e| e.to_string())?;

    Ok(())
}

pub(crate) fn write_lyrics_file(
    cache: &CacheDir,
    file_hash: &str,
    lines: &[String],
    line_anchors: Option<Vec<Option<f64>>>,
) -> std::io::Result<PathBuf> {
    let out = cache.lyrics_path(file_hash);
    let file = LyricsFile {
        lines: lines.to_vec(),
        line_anchors,
    };
    let json = serde_json::to_vec_pretty(&file).map_err(std::io::Error::other)?;
    std::fs::write(&out, json)?;
    Ok(out)
}

pub(crate) fn fetch_lrclib_lyrics(song: &Song, cache: &CacheDir) -> Option<PathBuf> {
    let existing = cache.lyrics_path(&song.file_hash);
    if existing.is_file() {
        info!(
            "[lrclib] Using existing lyrics file at {}",
            existing.display()
        );
        return Some(existing);
    }

    let candidates = lrclib_candidates(song);
    let pick = candidates.into_iter().next()?;

    info!(
        "[lrclib] Picked \"{}\" from \"{}\" (duration {:.0}s, delta {:.1}s)",
        pick.track_name,
        pick.album_name,
        pick.duration_secs,
        (pick.duration_secs - song.duration_secs).abs()
    );
    info!("[lrclib] Extracted {} lines", pick.lines.len());

    let anchors = pick
        .synced_lyrics
        .as_deref()
        .and_then(|synced| lrc::parse_lrc(synced).ok())
        .map(|parsed| match_synced_anchors(&pick.lines, &parsed))
        .and_then(anchors_if_sufficient);
    if let Some(ref a) = anchors {
        let matched = a.iter().filter(|x| x.is_some()).count();
        info!(
            "[lrclib] Timing anchors: {matched}/{} lines matched",
            a.len()
        );
    }

    match write_lyrics_file(cache, &song.file_hash, &pick.lines, anchors) {
        Ok(out) => {
            info!("[lrclib] Lyrics saved to {}", out.display());
            Some(out)
        }
        Err(e) => {
            warn!("[lrclib] Failed to write lyrics: {e}");
            None
        }
    }
}

/// Local lyrics sources, checked before falling back to the LRCLIB network
/// lookup (`fetch_lrclib_lyrics`): a `.lrc` sidecar next to the audio file,
/// then lyrics embedded directly in the file's own tags (ID3 `USLT` via
/// `ItemKey::UnsyncLyrics`, MP4 `©lyr` via `ItemKey::Lyrics` -- see
/// `read_embedded_lyrics`). Local is treated as
/// ground-truth-equivalent to the LRCLIB flow -- same shared cache file,
/// same downstream forced-alignment path (`align_lyrics` in align.py) once
/// `process_song` passes it through as `cmd_json["lyrics"]`.
///
/// Like `fetch_lrclib_lyrics`, this checks the shared cache first and reuses
/// it if present -- which is also what makes "local always wins" work
/// without extra bookkeeping: `process_song` calls this before
/// `fetch_lrclib_lyrics`, so whichever source is found first is the one
/// that gets cached, and the other call just sees the cache already there.
/// Separate LRCLIB lookup used only for timing anchors when a *local* source
/// (sidecar `.lrc` / embedded tag) has already won for display text, so
/// LRCLIB's synced timing isn't lost just because it lost the text race --
/// `process_song`'s `local_lyrics_path(...).or_else(|| fetch_lrclib_lyrics(...))`
/// short-circuits the display fetch in exactly this case, meaning
/// `fetch_lrclib_lyrics` (and its own anchor computation) never runs at all
/// when a local source exists. Silent `None` on any network failure or
/// no-match -- must never fail the analysis; `lrclib_candidates` already
/// degrades to an empty `Vec` on request failure.
fn fetch_lrclib_anchor_times(song: &Song, display_lines: &[String]) -> Option<Vec<Option<f64>>> {
    let pick = lrclib_candidates(song)
        .into_iter()
        .find(|c| c.synced_lyrics.is_some())?;
    let parsed = lrc::parse_lrc(pick.synced_lyrics.as_deref()?).ok()?;
    anchors_if_sufficient(match_synced_anchors(display_lines, &parsed))
}

pub(crate) fn local_lyrics_path(song: &Song, cache: &CacheDir) -> Option<PathBuf> {
    let existing = cache.lyrics_path(&song.file_hash);
    if existing.is_file() {
        return Some(existing);
    }

    let (source, raw_text) = read_sidecar_lrc(&song.path)
        .map(|text| ("sidecar .lrc", text))
        .or_else(|| read_embedded_lyrics(&song.path).map(|text| ("embedded tag", text)))?;

    let lines = lines_from_lyrics_text(&raw_text);
    if lines.is_empty() {
        return None;
    }

    info!(
        "[local-lyrics] Found {} lines via {source} for {}",
        lines.len(),
        song.path.display()
    );

    let anchors = fetch_lrclib_anchor_times(song, &lines);
    if let Some(ref a) = anchors {
        let matched = a.iter().filter(|x| x.is_some()).count();
        info!(
            "[local-lyrics] LRCLIB timing anchors: {matched}/{} lines matched",
            a.len()
        );
    }

    match write_lyrics_file(cache, &song.file_hash, &lines, anchors) {
        Ok(out) => {
            info!("[local-lyrics] Lyrics saved to {}", out.display());
            Some(out)
        }
        Err(e) => {
            warn!("[local-lyrics] Failed to write lyrics: {e}");
            None
        }
    }
}

fn read_sidecar_lrc(path: &Path) -> Option<String> {
    std::fs::read_to_string(path.with_extension("lrc")).ok()
}

fn read_embedded_lyrics(path: &Path) -> Option<String> {
    use lofty::file::TaggedFileExt;

    debug!("Reading tags: {}", path.display());
    let tagged = lofty::read_from_path(path).ok()?;
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag())?;
    // See `song::tag_has_lyrics` -- ID3v2 (MP3) only maps `USLT` to
    // `ItemKey::UnsyncLyrics`, never `ItemKey::Lyrics`.
    let text = tag
        .get_string(lofty::tag::ItemKey::Lyrics)
        .or_else(|| tag.get_string(lofty::tag::ItemKey::UnsyncLyrics))?;
    if text.trim().is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}

/// Plain lyric lines from raw text. Sidecar `.lrc` files are usually
/// timestamped, so try `lrc::parse_lrc` first (it also strips word-level
/// `<mm:ss.xx>` tags from Enhanced LRC); embedded tag lyrics are usually
/// plain text, so fall back to treating every non-empty line as-is.
fn lines_from_lyrics_text(text: &str) -> Vec<String> {
    if let Ok(parsed) = lrc::parse_lrc(text) {
        let lines: Vec<String> = parsed
            .segments
            .into_iter()
            .map(|s| s.text)
            .filter(|l| !l.trim().is_empty())
            .collect();
        if !lines.is_empty() {
            return lines;
        }
    }
    text.lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

