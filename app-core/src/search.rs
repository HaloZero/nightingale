//! Free-text ranked/typo-tolerant song matching, shared by two callers that
//! both need "closest song to this text" rather than a filtered list: the
//! Chromecast endpoint (`crate::chromecast`), turning a spoken/typed query
//! into a specific song, and the dedicated song-search page
//! (`search_songs_ranked`), doing typeahead across the whole library.
//! Unlike the library-menu `LIKE`-based search
//! (`library_db::queries::load_songs_page`, used by the main library
//! browser's search box and deliberately left alone), every candidate here
//! is ranked by similarity instead of filtered in/out by exact substring.

use crate::library_db;
use crate::song::Song;

/// Below this score, a Chromecast voice match is considered too weak to act
/// on automatically.
const MATCH_CONFIDENCE_THRESHOLD: f64 = 0.6;

/// Below this score, a search-page result is too weak to surface. Lower
/// than `MATCH_CONFIDENCE_THRESHOLD` since typeahead queries are typically
/// short partial prefixes typed character-by-character, not a full spoken
/// phrase.
const SEARCH_MIN_SCORE: f64 = 0.45;

fn normalize(s: &str) -> String {
    s.trim().to_lowercase()
}

/// Similarity between `query` (already normalized by the caller) and
/// `song`'s title/artist/album. Comparing word-by-word (rather than one
/// Jaro-Winkler call against the whole concatenated string) matters because
/// a short, possibly-typo'd query against a long multi-word title scores
/// poorly by whole-string similarity even when it's a near-perfect match for
/// one word in it -- e.g. "kathmandoo" vs "Kathmandu (Face It, You're
/// Caviar, I'm Hotdogs)" needs to compare against "kathmandu" alone, not the
/// full title. Every query word must find *some* good match somewhere in
/// the candidate's words (the `min` below) -- same AND-per-word requirement
/// as the substring search this replaces, just typo-tolerant per word
/// instead of exact. Floored high whenever `query` is a literal substring of
/// title/artist/album, so an exact or partial contains-match never ranks
/// below a merely-similar fuzzy one.
fn score_song(query: &str, song: &Song) -> f64 {
    let title = normalize(&song.title);
    let artist = normalize(&song.artist);
    let album = normalize(&song.album);
    let artist_title = format!("{artist} {title}");

    let candidate_words: Vec<&str> = title
        .split_whitespace()
        .chain(artist.split_whitespace())
        .chain(album.split_whitespace())
        .collect();

    let per_word = query
        .split_whitespace()
        .map(|query_word| {
            candidate_words
                .iter()
                .map(|candidate_word| strsim::jaro_winkler(query_word, candidate_word))
                .fold(0.0_f64, f64::max)
        })
        .fold(f64::INFINITY, f64::min);

    // A full-phrase comparison in addition to the per-word one: helps a
    // multi-word query that closely matches the whole "artist title" (e.g. a
    // spoken Chromecast command) outscore one that merely matches each word
    // in isolation.
    let whole_phrase = strsim::jaro_winkler(query, &artist_title)
        .max(strsim::jaro_winkler(query, &title))
        .max(strsim::jaro_winkler(query, &album));

    let fuzzy = per_word.max(whole_phrase);

    if title.contains(query) || artist_title.contains(query) || album.contains(query) {
        fuzzy.max(0.9)
    } else {
        fuzzy
    }
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

/// Ranked, typo-tolerant free-text search across the whole library (any
/// origin, any analysis status) for the dedicated search page. Unlike
/// `library_db::queries::load_songs_page`'s plain substring `LIKE` search
/// used by the main library browser -- intentionally left alone -- this
/// tolerates typos and orders by relevance instead of alphabetically.
pub fn search_songs_ranked(query: &str, limit: usize) -> Vec<Song> {
    let query = normalize(query);
    if query.is_empty() {
        return Vec::new();
    }

    let Ok(songs) = library_db::load_all_songs() else {
        return Vec::new();
    };

    let mut scored: Vec<(f64, Song)> = songs
        .into_iter()
        .map(|song| (score_song(&query, &song), song))
        .filter(|(score, _)| *score >= SEARCH_MIN_SCORE)
        .collect();

    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    scored.into_iter().take(limit).map(|(_, song)| song).collect()
}
