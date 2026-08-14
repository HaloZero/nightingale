//! Local-folder media source. Walks a directory tree with `walkdir`, classifies
//! audio/video/USDX files, and feeds the results into the library DB. This is
//! a direct refactor of the original `scanner.rs` logic.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use tracing::warn;
use walkdir::WalkDir;

use crate::cache::CacheDir;
use crate::config::AppConfig;
use crate::error::NightingaleError;
use crate::library_db::{self, PlaylistDefinition, PlaylistSongKeyKind};
use crate::song::{Song, SongOrigin, build_song, has_sidecar_lrc, tag_has_lyrics};
use crate::usdx;

use super::{MediaSource, SCAN_BATCH_SIZE, ScanContext, SourceKind, flush_batch};

const AUDIO_EXTENSIONS: &[&str] = &["mp3", "flac", "ogg", "opus", "wav", "m4a", "aac", "wma"];
const VIDEO_EXTENSIONS: &[&str] = &["mp4", "mkv", "avi", "webm", "mov", "m4v"];

#[derive(Debug, Clone, Copy)]
enum MediaKind {
    Audio,
    Video,
    Usdx,
}

pub struct FolderSource {
    root: PathBuf,
}

impl FolderSource {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

impl MediaSource for FolderSource {
    fn kind(&self) -> SourceKind {
        SourceKind::Folder
    }

    fn label(&self) -> String {
        self.root.to_string_lossy().into_owned()
    }

    fn scan(&self, ctx: &ScanContext<'_>) -> Result<(), NightingaleError> {
        let media_files = collect_media_paths(&self.root);
        let folder_label = self.label();

        // Source-switch wipes are handled centrally in `scanner::start_scan`;
        // here we only need to prune rows whose paths disappeared between
        // scans of the same folder.
        let paths: Vec<String> = media_files
            .iter()
            .map(|(p, _)| p.to_string_lossy().into_owned())
            .collect();
        let _ = library_db::delete_songs_not_in_paths(&paths);
        let _ = library_db::update_library_meta(&folder_label, media_files.len());

        // Loaded once and reused below for both "what's new" (the `pending`
        // filter) and the lyrics-flags refresh pass -- avoids a second
        // full-library query.
        let known_songs = library_db::load_all_songs().unwrap_or_default();
        let already_processed: HashSet<String> = known_songs
            .iter()
            .map(|s| s.path.to_string_lossy().into_owned())
            .collect();

        let pending: Vec<_> = media_files
            .into_iter()
            .filter(|(p, _)| !already_processed.contains(&p.to_string_lossy().into_owned()))
            .collect();

        let mut batch: Vec<Song> = Vec::new();
        let generation = ctx.generation;

        for (i, (path, kind)) in pending.iter().enumerate() {
            if !library_db::scan_generation_is_current(generation) {
                return Ok(());
            }
            let result = match kind {
                MediaKind::Audio => build_song(path, ctx.cache, false),
                MediaKind::Video => build_song(path, ctx.cache, true),
                MediaKind::Usdx => usdx::build_usdx_song(path, ctx.cache),
            };
            match result {
                Ok(song) => batch.push(song),
                Err(e) => {
                    warn!("Failed to process {}: {e}", path.display());
                }
            }
            if (i + 1) % SCAN_BATCH_SIZE == 0 {
                flush_batch(&mut batch, generation);
            }
        }

        flush_batch(&mut batch, generation);

        if library_db::scan_generation_is_current(generation)
            && AppConfig::load().refresh_lyrics_on_scan()
        {
            refresh_lyrics_flags(&known_songs, generation);
        }

        if library_db::scan_generation_is_current(generation) {
            sync_folder_playlists(&self.root);
        }
        Ok(())
    }

    fn ensure_local_media(
        &self,
        song: &Song,
        _cache: &CacheDir,
    ) -> Result<PathBuf, NightingaleError> {
        Ok(song.path.clone())
    }
}

fn classify_media_file(path: &Path) -> Option<MediaKind> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase());

    let ext_str = ext.as_deref()?;

    if AUDIO_EXTENSIONS.contains(&ext_str) {
        Some(MediaKind::Audio)
    } else if VIDEO_EXTENSIONS.contains(&ext_str) {
        Some(MediaKind::Video)
    } else if ext_str == "usdx" {
        Some(MediaKind::Usdx)
    } else if ext_str == "txt" && usdx::looks_like_usdx(path) {
        Some(MediaKind::Usdx)
    } else {
        None
    }
}

/// A regular scan only calls `build_song` for brand-new paths (see the
/// `already_processed` filter in `scan` above), so an already-known song is
/// otherwise frozen at whatever was true the first time it was scanned.
/// `has_lrc_file`/`has_embedded_lyrics` are the deliberate exception: a
/// `.lrc` sidecar can show up, or tags can get edited, well after a song was
/// first added, and both are cheap to re-check (a `stat` + one tag read) --
/// so every scan/rescan re-derives them for every already-known local song
/// and only writes a DB row when something actually changed. USDX songs are
/// skipped: their lyrics come from the chart file itself, not a sidecar/tag
/// next to it (see usdx.rs), so re-checking their `path` would be checking
/// the wrong file. Remote-source songs are skipped too -- no local bytes to
/// check at scan time.
fn refresh_lyrics_flags(known_songs: &[Song], generation: u64) {
    for song in known_songs {
        if !library_db::scan_generation_is_current(generation) {
            return;
        }
        if !matches!(song.origin, SongOrigin::LocalFile) || song.usdx.is_some() {
            continue;
        }
        let has_lrc_file = has_sidecar_lrc(&song.path);
        let has_embedded_lyrics = !song.is_video && tag_has_lyrics(&song.path);
        if has_lrc_file == song.has_lrc_file && has_embedded_lyrics == song.has_embedded_lyrics {
            continue;
        }
        let mut updated = song.clone();
        updated.has_lrc_file = has_lrc_file;
        updated.has_embedded_lyrics = has_embedded_lyrics;
        if let Err(e) = library_db::update_song_fields(&song.file_hash, &updated) {
            warn!(
                "Failed to refresh lyrics flags for {}: {e}",
                song.path.display()
            );
        }
    }
}

fn sync_folder_playlists(root: &Path) {
    let song_paths = library_db::load_song_path_strings().unwrap_or_default();
    let canonical_song_paths: HashMap<PathBuf, String> = song_paths
        .into_iter()
        .filter_map(|stored| {
            std::fs::canonicalize(&stored)
                .ok()
                .map(|canonical| (canonical, stored))
        })
        .collect();

    let mut playlists = Vec::new();
    for path in collect_playlist_paths(root) {
        let Ok(bytes) = std::fs::read(&path) else {
            warn!("Failed to read playlist {}", path.display());
            continue;
        };
        let content = String::from_utf8_lossy(&bytes);
        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let entries = if extension == "pls" {
            parse_pls_entries(&content)
        } else {
            parse_m3u_entries(&content)
        };
        let base = path.parent().unwrap_or(root);
        let song_keys = entries
            .into_iter()
            .filter_map(|entry| resolve_playlist_entry(base, &entry))
            .filter_map(|canonical| canonical_song_paths.get(&canonical).cloned())
            .collect();
        let id_path = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        let name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("Playlist")
            .to_string();
        playlists.push(PlaylistDefinition {
            id: format!("folder:{}", id_path.to_string_lossy()),
            name,
            song_keys,
        });
    }

    if let Err(error) =
        library_db::replace_all_playlists(&playlists, PlaylistSongKeyKind::LocalPath)
    {
        warn!("Failed to sync folder playlists: {error}");
    }
}

fn collect_playlist_paths(folder: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<_> = WalkDir::new(folder)
        .follow_links(true)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_file())
        .filter_map(|entry| {
            let extension = entry.path().extension()?.to_str()?.to_ascii_lowercase();
            matches!(extension.as_str(), "m3u" | "m3u8" | "pls").then(|| entry.path().to_path_buf())
        })
        .collect();
    paths.sort();
    paths
}

fn parse_m3u_entries(content: &str) -> Vec<String> {
    content
        .lines()
        .map(|line| line.trim().trim_start_matches('\u{feff}'))
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect()
}

fn parse_pls_entries(content: &str) -> Vec<String> {
    let mut entries: Vec<(usize, String)> = content
        .lines()
        .filter_map(|line| {
            let (key, value) = line.trim().split_once('=')?;
            let key = key.to_ascii_lowercase();
            let index = key.strip_prefix("file")?.parse::<usize>().ok()?;
            let value = value.trim();
            (!value.is_empty()).then(|| (index, value.to_string()))
        })
        .collect();
    entries.sort_by_key(|(index, _)| *index);
    entries.into_iter().map(|(_, value)| value).collect()
}

fn resolve_playlist_entry(base: &Path, entry: &str) -> Option<PathBuf> {
    let entry = entry.trim().trim_matches('"');
    let path = Path::new(entry);
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        if entry.contains("://") {
            return None;
        }
        base.join(path)
    };
    std::fs::canonicalize(candidate).ok()
}

fn collect_media_paths(folder: &Path) -> Vec<(PathBuf, MediaKind)> {
    let mut paths: Vec<(PathBuf, MediaKind)> = WalkDir::new(folder)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .filter_map(|e| {
            let kind = classify_media_file(e.path())?;
            Some((e.path().to_path_buf(), kind))
        })
        .collect();

    let claimed: HashSet<PathBuf> = paths
        .iter()
        .filter_map(|(p, kind)| matches!(kind, MediaKind::Usdx).then(|| p.clone()))
        .filter_map(|usdx_path| usdx::read_siblings(&usdx_path))
        .flat_map(|s| {
            [Some(s.audio), s.vocals, s.instrumental, s.video]
                .into_iter()
                .flatten()
        })
        .collect();

    paths.retain(|(p, kind)| matches!(kind, MediaKind::Usdx) || !claimed.contains(p));
    paths
}
