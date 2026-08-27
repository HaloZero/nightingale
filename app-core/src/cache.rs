use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Deserialize, Default, TS)]
#[ts(export)]
pub struct CachePaths {
    pub songs: Option<PathBuf>,
    pub videos: Option<PathBuf>,
    pub models: Option<PathBuf>,
    pub vendor: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct CacheDir {
    pub path: PathBuf,
}

impl CacheDir {
    pub fn new() -> Self {
        let path = songs_cache_dir();
        std::fs::create_dir_all(&path).expect("could not create cache directory");
        Self { path }
    }

    pub fn transcript_path(&self, hash: &str) -> PathBuf {
        self.path.join(format!("{hash}_transcript.json"))
    }

    pub fn variant_transcript_path(&self, hash: &str, tempo: f64) -> PathBuf {
        self.path
            .join(format!("{hash}_transcript_{}.json", format_tempo(tempo)))
    }

    pub fn instrumental_path(&self, hash: &str) -> PathBuf {
        self.path.join(format!("{hash}_instrumental.mp3"))
    }

    pub fn vocals_path(&self, hash: &str) -> PathBuf {
        self.path.join(format!("{hash}_vocals.mp3"))
    }

    pub fn variant_instrumental_path(&self, hash: &str, key: &str, tempo: f64) -> PathBuf {
        self.path.join(format!(
            "{hash}_instrumental_{}_{}.mp3",
            sanitize_key(key),
            format_tempo(tempo)
        ))
    }

    pub fn variant_vocals_path(&self, hash: &str, key: &str, tempo: f64) -> PathBuf {
        self.path.join(format!(
            "{hash}_vocals_{}_{}.mp3",
            sanitize_key(key),
            format_tempo(tempo)
        ))
    }

    pub fn legacy_instrumental_path(&self, hash: &str) -> PathBuf {
        self.path.join(format!("{hash}_instrumental.ogg"))
    }

    pub fn legacy_vocals_path(&self, hash: &str) -> PathBuf {
        self.path.join(format!("{hash}_vocals.ogg"))
    }

    fn stems_exist(&self, hash: &str) -> bool {
        (self.instrumental_path(hash).is_file() && self.vocals_path(hash).is_file())
            || (self.legacy_instrumental_path(hash).is_file()
                && self.legacy_vocals_path(hash).is_file())
            || self.has_variant_stems(hash)
    }

    pub fn has_variant_stems(&self, hash: &str) -> bool {
        let Ok(entries) = std::fs::read_dir(&self.path) else {
            return false;
        };

        let inst_prefix = format!("{hash}_instrumental_");
        let voc_prefix = format!("{hash}_vocals_");
        let mut inst_suffixes = std::collections::HashSet::new();
        let mut voc_suffixes = std::collections::HashSet::new();

        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if let Some(suffix) = stem_suffix(&name, &inst_prefix) {
                inst_suffixes.insert(suffix.to_string());
            } else if let Some(suffix) = stem_suffix(&name, &voc_prefix) {
                voc_suffixes.insert(suffix.to_string());
            }
        }

        inst_suffixes.iter().any(|s| voc_suffixes.contains(s))
    }

    pub fn lyrics_path(&self, hash: &str) -> PathBuf {
        self.path.join(format!("{hash}_lyrics.json"))
    }

    pub fn cover_path(&self, hash: &str) -> PathBuf {
        self.path.join(format!("{hash}_cover.jpg"))
    }

    pub fn playable_video_path(&self, hash: &str) -> PathBuf {
        let dir = self.path.join("playable_videos");
        std::fs::create_dir_all(&dir).ok();
        dir.join(format!("{hash}.mp4"))
    }

    /// The reel/raw-clip-background karaoke video -- see `karaoke_video::
    /// ensure_karaoke_video`. Kept separate from `youtube_karaoke_video_path`
    /// so both flavors can exist at once and `karaoke_video::
    /// best_karaoke_video_path` can pick between them, rather than one
    /// slot's render overwriting whichever background the other used.
    pub fn karaoke_video_path(&self, hash: &str) -> PathBuf {
        let dir = self.path.join("karaoke_videos");
        std::fs::create_dir_all(&dir).ok();
        dir.join(format!("{hash}.mp4"))
    }

    /// The YouTube-video-background karaoke video -- see `karaoke_video::
    /// ensure_youtube_background_karaoke_video`. Sibling to
    /// `karaoke_video_path`, same directory, distinguished by suffix so a
    /// directory listing shows both flavors for a song side by side.
    pub fn youtube_karaoke_video_path(&self, hash: &str) -> PathBuf {
        let dir = self.path.join("karaoke_videos");
        std::fs::create_dir_all(&dir).ok();
        dir.join(format!("{hash}_youtube.mp4"))
    }

    /// Downloaded official YouTube music video for a song (`youtube_video::
    /// ensure_youtube_video_downloaded`), kept separate from
    /// `karaoke_video_path` -- this is the raw source, not the rendered
    /// karaoke output built from it.
    pub fn youtube_video_path(&self, hash: &str) -> PathBuf {
        let dir = self.path.join("youtube_videos");
        std::fs::create_dir_all(&dir).ok();
        dir.join(format!("{hash}.mp4"))
    }

    pub fn transcript_exists(&self, hash: &str) -> bool {
        let transcript = self.transcript_path(hash);
        if !transcript.is_file() {
            return false;
        }
        // A transcript built from provided LRC without stem separation is
        // complete on its own; stems are only required for the normal pipeline.
        self.stems_exist(hash) || transcript_marks_no_stems(&transcript)
    }

    pub fn delete_song_cache(&self, hash: &str) {
        for path in [
            self.transcript_path(hash),
            self.instrumental_path(hash),
            self.vocals_path(hash),
            self.legacy_instrumental_path(hash),
            self.legacy_vocals_path(hash),
            self.lyrics_path(hash),
            self.playable_video_path(hash),
        ] {
            if path.is_file() {
                let _ = std::fs::remove_file(&path);
            }
        }

        if let Ok(entries) = std::fs::read_dir(&self.path) {
            for entry in entries.flatten() {
                let path = entry.path();
                let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };
                if name.starts_with(&format!("{hash}_instrumental_"))
                    || name.starts_with(&format!("{hash}_vocals_"))
                    || is_variant_transcript_file(name, hash)
                {
                    let _ = std::fs::remove_file(&path);
                }
            }
        }
    }

    pub fn delete_transcript_variants(&self, hash: &str) {
        if let Ok(entries) = std::fs::read_dir(&self.path) {
            for entry in entries.flatten() {
                let path = entry.path();
                let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };
                if is_variant_transcript_file(name, hash) {
                    let _ = std::fs::remove_file(&path);
                }
            }
        }
    }

    pub fn clear_all(&self) {
        if self.path.is_dir() {
            let _ = std::fs::remove_dir_all(&self.path);
            let _ = std::fs::create_dir_all(&self.path);
        }
    }
}

fn stem_suffix<'a>(name: &'a str, prefix: &str) -> Option<&'a str> {
    name.strip_prefix(prefix)
        .and_then(|tail| tail.strip_suffix(".mp3"))
}

/// True when a transcript file was built from provided LRC without stem
/// separation (`"no_stems": true`), meaning it is playable without stems.
fn transcript_marks_no_stems(path: &Path) -> bool {
    #[derive(serde::Deserialize)]
    struct NoStemsProbe {
        #[serde(default)]
        no_stems: bool,
    }
    std::fs::read_to_string(path)
        .ok()
        .and_then(|data| serde_json::from_str::<NoStemsProbe>(&data).ok())
        .map(|probe| probe.no_stems)
        .unwrap_or(false)
}

fn is_variant_transcript_file(name: &str, hash: &str) -> bool {
    name.starts_with(&format!("{hash}_transcript_")) && name.ends_with(".json")
}

pub fn sanitize_key(key: &str) -> String {
    let mut out = String::with_capacity(key.len());
    for ch in key.trim().chars() {
        if ch.is_ascii_alphanumeric() || ch == '#' || ch == 'b' {
            out.push(ch);
        } else if ch == ' ' || ch == '-' || ch == '_' {
            out.push('_');
        }
    }
    let cleaned = out.trim_matches('_').replace("__", "_");
    if cleaned.is_empty() {
        "Unknown".to_string()
    } else {
        cleaned
    }
}

pub fn normalize_tempo(tempo: f64) -> f64 {
    if !tempo.is_finite() || tempo <= 0.0 {
        1.0
    } else {
        (tempo * 10.0).round() / 10.0
    }
}

pub fn format_tempo(tempo: f64) -> String {
    format!("{:.1}", normalize_tempo(tempo))
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, TS)]
#[ts(export)]
pub struct CacheStats {
    pub songs_bytes: u64,
    pub videos_bytes: u64,
    pub models_bytes: u64,
    pub other_bytes: u64,
    pub clearable_videos_bytes: u64,
}

impl CacheStats {
    pub fn calculate() -> Self {
        let base = nightingale_dir();

        let songs_bytes = dir_size(&songs_cache_dir());
        let videos_bytes = dir_size(&videos_dir());
        let models_bytes = dir_size(&models_dir());
        let other_bytes = dir_size(&vendor_dir())
            + dir_size(&base.join("sounds"))
            + default_nightingale_dir()
                .join("nightingale.log")
                .metadata()
                .map(|m| m.len())
                .unwrap_or(0)
            + config_path().metadata().map(|m| m.len()).unwrap_or(0)
            + base
                .join("profiles.json")
                .metadata()
                .map(|m| m.len())
                .unwrap_or(0);

        Self {
            songs_bytes,
            videos_bytes,
            models_bytes,
            other_bytes,
            clearable_videos_bytes: clearable_video_bytes(),
        }
    }
}

pub fn nightingale_dir() -> PathBuf {
    configured_data_path().unwrap_or_else(default_nightingale_dir)
}

pub fn default_nightingale_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("NIGHTINGALE_DATA_PATH") {
        let p = PathBuf::from(path);

        if !p.as_os_str().is_empty() {
            return p;
        }
    }

    dirs::home_dir()
        .expect("could not find home directory")
        .join(".nightingale")
}

pub fn config_path() -> PathBuf {
    default_nightingale_dir().join("config.json")
}

pub fn profiles_path() -> PathBuf {
    nightingale_dir().join("profiles.json")
}

pub fn songs_path() -> PathBuf {
    nightingale_dir().join("songs.json")
}

pub fn analysis_queue_path() -> PathBuf {
    nightingale_dir().join("analysis_queue.json")
}

pub fn songs_cache_dir() -> PathBuf {
    configured_cache_paths()
        .songs
        .unwrap_or_else(|| nightingale_dir().join("cache"))
}

pub fn videos_dir() -> PathBuf {
    configured_cache_paths()
        .videos
        .unwrap_or_else(|| nightingale_dir().join("videos"))
}

/// Pre-built background "reels" for karaoke video (`playback::
/// build_background_reels`) -- each one several raw clips of `flavor`
/// concatenated together, sibling to the raw single-clip cache at
/// `videos_dir()/<flavor>`.
pub fn reels_dir(flavor: &str) -> PathBuf {
    let dir = videos_dir().join(format!("{flavor}_reels"));
    std::fs::create_dir_all(&dir).ok();
    dir
}

/// One normalized (scaled/cropped/fps-matched, already-encoded) copy of each
/// raw `flavor` clip, keyed by filename -- `playback::build_one_reel` reuses
/// these across every reel that picks a given clip instead of re-decoding
/// and re-encoding it from the raw source each time. Sibling to
/// `reels_dir(flavor)`.
pub fn normalized_clips_dir(flavor: &str) -> PathBuf {
    let dir = videos_dir().join(format!("{flavor}_normalized"));
    std::fs::create_dir_all(&dir).ok();
    dir
}

pub fn models_dir() -> PathBuf {
    configured_cache_paths()
        .models
        .unwrap_or_else(|| nightingale_dir().join("models"))
}

pub fn vendor_dir() -> PathBuf {
    configured_cache_paths()
        .vendor
        .unwrap_or_else(|| nightingale_dir().join("vendor"))
}

pub fn cache_roots() -> Vec<PathBuf> {
    vec![songs_cache_dir(), videos_dir(), models_dir(), vendor_dir()]
}

pub fn dir_size(path: &Path) -> u64 {
    if !path.is_dir() {
        return 0;
    }

    WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}

pub fn clearable_video_bytes() -> u64 {
    let base = videos_dir();

    if !base.is_dir() {
        return 0;
    }

    let mut total: u64 = 0;
    for entry in std::fs::read_dir(&base).into_iter().flatten().flatten() {
        let flavor_dir = entry.path();

        if !flavor_dir.is_dir() {
            continue;
        }

        let mut mp4s: Vec<_> = std::fs::read_dir(&flavor_dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|ext| ext == "mp4"))
            .collect();
        mp4s.sort();

        for path in mp4s.into_iter().skip(1) {
            total += path.metadata().map(|m| m.len()).unwrap_or(0);
        }
    }
    total
}

pub fn clear_videos() {
    let base = videos_dir();

    if !base.is_dir() {
        return;
    }

    for entry in std::fs::read_dir(&base).into_iter().flatten().flatten() {
        let flavor_dir = entry.path();
        if !flavor_dir.is_dir() {
            continue;
        }
        let mut mp4s: Vec<_> = std::fs::read_dir(&flavor_dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|ext| ext == "mp4"))
            .collect();

        mp4s.sort();

        for path in mp4s.into_iter().skip(1) {
            let _ = std::fs::remove_file(&path);
        }
    }
}

pub fn clear_models() {
    let dir = models_dir();

    if dir.is_dir() {
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[derive(Debug, Deserialize)]
struct DataPathOnlyConfig {
    data_path: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
struct PathsOnlyConfig {
    cache_paths: Option<CachePaths>,
}

fn resolve_configured_path(path: PathBuf) -> Option<PathBuf> {
    if path.as_os_str().is_empty() {
        return None;
    }

    if path.is_absolute() {
        Some(path)
    } else {
        std::env::current_dir().ok().map(|cwd| cwd.join(path))
    }
}

fn configured_data_path() -> Option<PathBuf> {
    let path = config_path();
    let content = std::fs::read_to_string(path).ok()?;
    let configured = serde_json::from_str::<DataPathOnlyConfig>(&content)
        .ok()
        .and_then(|cfg| cfg.data_path)?;
    resolve_configured_path(configured)
}

fn configured_cache_paths() -> CachePaths {
    let path = config_path();
    let Some(content) = std::fs::read_to_string(path).ok() else {
        return CachePaths::default();
    };
    let Some(paths) = serde_json::from_str::<PathsOnlyConfig>(&content)
        .ok()
        .and_then(|cfg| cfg.cache_paths)
    else {
        return CachePaths::default();
    };

    CachePaths {
        songs: paths.songs.and_then(resolve_configured_path),
        videos: paths.videos.and_then(resolve_configured_path),
        models: paths.models.and_then(resolve_configured_path),
        vendor: paths.vendor.and_then(resolve_configured_path),
    }
}

pub fn normalized_target_path(path: PathBuf) -> Result<PathBuf, String> {
    if path.as_os_str().is_empty() {
        return Err("data_path cannot be empty".to_string());
    }

    if path.is_absolute() {
        Ok(path)
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .map_err(|e| format!("failed to resolve relative data_path: {e}"))
    }
}

pub fn same_path(lhs: &Path, rhs: &Path) -> bool {
    match (
        std::fs::canonicalize(lhs).ok(),
        std::fs::canonicalize(rhs).ok(),
    ) {
        (Some(a), Some(b)) => a == b,
        _ => lhs == rhs,
    }
}

fn copy_path_entry(src: &Path, dst: &Path) -> Result<(), String> {
    let metadata = std::fs::symlink_metadata(src)
        .map_err(|e| format!("failed reading metadata for {:?}: {e}", src))?;
    let file_type = metadata.file_type();

    if file_type.is_dir() {
        std::fs::create_dir_all(dst)
            .map_err(|e| format!("failed creating destination directory {:?}: {e}", dst))?;
        for child in std::fs::read_dir(src)
            .map_err(|e| format!("failed reading directory {:?}: {e}", src))?
        {
            let child = child.map_err(|e| format!("failed reading directory entry: {e}"))?;
            let child_src = child.path();
            let child_dst = dst.join(child.file_name());
            copy_path_entry(&child_src, &child_dst)?;
        }
        return Ok(());
    }

    if file_type.is_symlink() {
        if dst.exists() {
            if dst.is_dir() {
                std::fs::remove_dir_all(dst)
                    .map_err(|e| format!("failed clearing destination {:?}: {e}", dst))?;
            } else {
                std::fs::remove_file(dst)
                    .map_err(|e| format!("failed clearing destination {:?}: {e}", dst))?;
            }
        } else if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("failed creating destination parent {:?}: {e}", parent))?;
        }

        let link_target = std::fs::read_link(src)
            .map_err(|e| format!("failed reading symlink {:?}: {e}", src))?;
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&link_target, dst)
                .map_err(|e| format!("failed creating symlink {:?}: {e}", dst))?;
        }
        #[cfg(windows)]
        {
            let target_is_dir = src.is_dir();
            if target_is_dir {
                std::os::windows::fs::symlink_dir(&link_target, dst)
                    .map_err(|e| format!("failed creating symlink dir {:?}: {e}", dst))?;
            } else {
                std::os::windows::fs::symlink_file(&link_target, dst)
                    .map_err(|e| format!("failed creating symlink file {:?}: {e}", dst))?;
            }
        }
        return Ok(());
    }

    if dst.exists() {
        if dst.is_dir() {
            std::fs::remove_dir_all(dst)
                .map_err(|e| format!("failed clearing destination {:?}: {e}", dst))?;
        } else {
            std::fs::remove_file(dst)
                .map_err(|e| format!("failed clearing destination {:?}: {e}", dst))?;
        }
    } else if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed creating destination parent {:?}: {e}", parent))?;
    }

    std::fs::copy(src, dst).map_err(|e| format!("failed copying {:?} -> {:?}: {e}", src, dst))?;
    Ok(())
}

fn migrate_data_entries_with<F>(
    old_root: &Path,
    new_root: &Path,
    copy_entry: F,
) -> Result<Vec<std::ffi::OsString>, String>
where
    F: Fn(&Path, &Path) -> Result<(), String>,
{
    let mut migrated = Vec::new();
    if !old_root.is_dir() {
        return Ok(migrated);
    }

    for entry in std::fs::read_dir(old_root)
        .map_err(|e| format!("failed reading current data path {:?}: {e}", old_root))?
    {
        let entry = entry.map_err(|e| format!("failed reading data path entry: {e}"))?;
        let name = entry.file_name();
        let entry_name = name.to_string_lossy();
        if entry_name == "config.json" || entry_name == "nightingale.log" {
            continue;
        }

        let src = entry.path();
        let dst = new_root.join(&name);
        copy_entry(&src, &dst)?;
        migrated.push(name);
    }

    Ok(migrated)
}

fn cleanup_migrated_source_entries(old_root: &Path, migrated: &[std::ffi::OsString]) {
    for name in migrated {
        let src = old_root.join(name);
        if src.is_dir() {
            let _ = std::fs::remove_dir_all(&src);
        } else if src.exists() {
            let _ = std::fs::remove_file(&src);
        }
    }
}

pub fn migrate_directory_contents(old_root: &Path, new_root: &Path) -> Result<(), String> {
    if same_path(old_root, new_root) || !old_root.is_dir() {
        std::fs::create_dir_all(new_root)
            .map_err(|e| format!("failed creating cache path {:?}: {e}", new_root))?;
        return Ok(());
    }

    if new_root.starts_with(old_root) {
        return Err("new cache path cannot be inside current cache path".to_string());
    }

    std::fs::create_dir_all(new_root)
        .map_err(|e| format!("failed creating cache path {:?}: {e}", new_root))?;
    let migrated =
        migrate_data_entries_with(old_root, new_root, |src, dst| copy_path_entry(src, dst))?;
    cleanup_migrated_source_entries(old_root, &migrated);

    Ok(())
}

pub fn change_app_data_path(new_path: PathBuf) -> Result<PathBuf, String> {
    let old_root = nightingale_dir();
    let new_root = normalized_target_path(new_path)?;

    if same_path(&old_root, &new_root) {
        let default_root = default_nightingale_dir();
        if !same_path(&new_root, &default_root) {
            crate::library_db::rebase_song_album_art_paths(&default_root, &new_root)?;
        }
        let mut cfg = crate::config::AppConfig::load();
        cfg.data_path = Some(new_root.clone());
        cfg.save();
        crate::library_db::reconnect_library_at_root(&new_root)?;
        return Ok(new_root);
    }

    if new_root.starts_with(&old_root) {
        return Err("new data_path cannot be inside current data path".to_string());
    }

    std::fs::create_dir_all(&new_root)
        .map_err(|e| format!("failed creating new data path {:?}: {e}", new_root))?;
    let migrated =
        migrate_data_entries_with(&old_root, &new_root, |src, dst| copy_path_entry(src, dst))?;

    crate::library_db::rebase_song_album_art_paths(&old_root, &new_root)?;
    crate::library_db::reconnect_library_at_root(&new_root)?;

    let mut cfg = crate::config::AppConfig::load();
    cfg.data_path = Some(new_root.clone());
    cfg.save();
    cleanup_migrated_source_entries(&old_root, &migrated);

    Ok(new_root)
}
