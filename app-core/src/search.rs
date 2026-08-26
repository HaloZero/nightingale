//! Free-text "best matching song" lookup, used by the Chromecast endpoint
//! (`crate::chromecast`) to turn a spoken/typed query into a specific song.
//! Unlike the library-menu `LIKE`-based search (`library_db::load_songs_page`),
//! this ranks every candidate by similarity so a single best match can be
//! picked instead of a filtered list.

use crate::library_db;
use crate::song::Song;

/// Below this score, a match is considered too weak to act on automatically.
const MATCH_CONFIDENCE_THRESHOLD: f64 = 0.6;

fn normalize(s: &str) -> String {
    s.trim().to_lowercase()
}

fn score_song(query: &str, song: &Song) -> f64 {
    let title_score = strsim::jaro_winkler(query, &normalize(&song.title));
    let artist_title = normalize(&format!("{} {}", song.artist, song.title));
    let artist_title_score = strsim::jaro_winkler(query, &artist_title);
    title_score.max(artist_title_score)
}

/// Best `SongOrigin::LocalFile` match for `query`, or `None` if the library
/// has no local songs or nothing scores above `MATCH_CONFIDENCE_THRESHOLD`.
/// Matches regardless of analysis status -- callers that require a castable
/// (analyzed) song must check `Song.is_analyzed` themselves, see
/// `find_alternative_analyzed_songs` for a same-query fallback list.
pub fn find_best_matching_local_song(query: &str) -> Option<Song> {
    let query = normalize(query);
    if query.is_empty() {
        return None;
    }

    let songs = library_db::load_all_local_songs().ok()?;
    let (best_song, _best_score) = songs.into_iter().fold(
        (None, MATCH_CONFIDENCE_THRESHOLD),
        |(best_song, best_score), song| {
            let score = score_song(&query, &song);
            if score >= best_score {
                (Some(song), score)
            } else {
                (best_song, best_score)
            }
        },
    );

    best_song
}

/// Direct, unambiguous lookup by `file_hash` -- used for the "cast this
/// exact song" links offered when a fuzzy `find_best_matching_local_song`
/// match turns out not to be analyzed yet, so re-clicking one of those
/// links can't land on a *different* song the way a re-run of the fuzzy
/// text search theoretically could (e.g. if the library changed in
/// between).
pub fn find_song_by_hash(file_hash: &str) -> Option<Song> {
    library_db::load_song_by_hash(file_hash).ok().flatten()
}

/// Every analyzed local song ranked by similarity to `query`, most similar
/// first, capped at `limit` -- no confidence floor (unlike
/// `find_best_matching_local_song`), since this only runs once we already
/// know the real best match isn't castable (not analyzed yet) and even a
/// low-confidence "closest analyzed song" is a more useful suggestion than
/// nothing.
pub fn find_alternative_analyzed_songs(query: &str, limit: usize) -> Vec<Song> {
    let query = normalize(query);
    let Ok(songs) = library_db::load_all_local_songs() else {
        return Vec::new();
    };

    let mut scored: Vec<(f64, Song)> = songs
        .into_iter()
        .filter(|song| song.is_analyzed)
        .map(|song| {
            let score = if query.is_empty() { 0.0 } else { score_song(&query, &song) };
            (score, song)
        })
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    scored.into_iter().take(limit).map(|(_, song)| song).collect()
}
