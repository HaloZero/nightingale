//! Throwaway benchmark: seeds real (non-synthetic) transcript data for
//! whichever sample songs under `./songs` we have matching ASR output for
//! (`bench_out/transcripts/<slug>/whisper_*.json`, already in production
//! transcript shape), then times `ensure_karaoke_video` for each.
//!
//! Run from the workspace root: `cargo run -p app-core --release --example karaoke_video_bench`

use std::path::{Path, PathBuf};
use std::time::Instant;

fn slugify(s: &str) -> String {
    let mut out = String::new();
    let mut last_was_sep = true;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            last_was_sep = false;
        } else if !last_was_sep {
            out.push('_');
            last_was_sep = true;
        }
    }
    while out.ends_with('_') {
        out.pop();
    }
    out
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("app-core must live one level below the workspace root")
        .to_path_buf()
}

fn main() {
    let root = workspace_root();
    let songs_dir = root.join("songs");
    let transcripts_dir = root.join("bench_out").join("transcripts");

    let scratch = std::env::temp_dir().join(format!(
        "nightingale-karaoke-bench-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&scratch).expect("create scratch data dir");
    // Safety: single-threaded example, set before any app_core call reads it.
    unsafe { std::env::set_var("NIGHTINGALE_DATA_PATH", &scratch) };

    // Verification-only: seed this fresh scratch dir's nature cache from an
    // already-downloaded directory (e.g. a `pixabay_bulk_download` run) so
    // this bench exercises the video-background overlay path instead of
    // falling back to solid color. No-op if the env var isn't set.
    if let Ok(src) = std::env::var("SEED_NATURE_VIDEOS_FROM") {
        let dest = scratch.join("videos").join("nature");
        std::fs::create_dir_all(&dest).expect("create nature videos dir");
        let mut copied = 0;
        for entry in std::fs::read_dir(&src).into_iter().flatten().flatten().take(5) {
            let name = entry.file_name();
            std::fs::copy(entry.path(), dest.join(&name)).expect("copy seed nature video");
            copied += 1;
        }
        println!("seeded {copied} nature videos from {src}");
    }

    app_core::startup().expect("app_core startup");

    // This benchmark only needs an ffmpeg with libx264 (we render text
    // ourselves and use ffmpeg purely as an encoder -- no libass/
    // drawtext dependency), so reuse whatever `ffmpeg` is on PATH instead
    // of running the full vendor download step.
    let vendor_dir = app_core::nightingale_dir().join("vendor");
    std::fs::create_dir_all(&vendor_dir).expect("create vendor dir");
    let system_ffmpeg = String::from_utf8(
        std::process::Command::new("which")
            .arg("ffmpeg")
            .output()
            .expect("locate system ffmpeg")
            .stdout,
    )
    .unwrap()
    .trim()
    .to_string();
    if system_ffmpeg.is_empty() {
        eprintln!("no ffmpeg found on PATH; install one (e.g. `brew install ffmpeg`) to run this bench");
        std::process::exit(1);
    }
    let vendored_ffmpeg = vendor_dir.join("ffmpeg");
    std::fs::copy(&system_ffmpeg, &vendored_ffmpeg).expect("copy ffmpeg into vendor dir");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&vendored_ffmpeg, std::fs::Permissions::from_mode(0o755))
            .expect("chmod ffmpeg");
    }

    let mut config = app_core::AppConfig::load();
    config.library_source = Some(app_core::LibrarySource::Folder {
        path: songs_dir.clone(),
    });
    config.save();
    app_core::start_scan();

    println!("scanning {} ...", songs_dir.display());
    let expected_min = 5;
    let mut songs = Vec::new();
    for _ in 0..60 {
        std::thread::sleep(std::time::Duration::from_millis(500));
        let store = app_core::SongsStore::load(&app_core::LoadSongsParams {
            search: None,
            filters: app_core::LibraryMenuFilters::default(),
            skip: 0,
            take: 1000,
        });
        if store.processed.len() >= expected_min {
            songs = store.processed;
            break;
        }
    }
    if songs.is_empty() {
        eprintln!("scan did not find any songs under {}", songs_dir.display());
        std::process::exit(1);
    }
    println!("scanned {} songs", songs.len());

    let available_slugs: Vec<String> = std::fs::read_dir(&transcripts_dir)
        .expect("read bench_out/transcripts")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();

    let cache = app_core::CacheDir::new();
    let mut matched: Vec<(String, app_core::Song)> = Vec::new();
    for song in songs {
        let slug = slugify(&song.title);
        // Exact match first, then substring either direction -- catches
        // near-misses like "all_the_small_thing" (typo'd file) vs. the
        // bench transcript's "all_the_small_things", or a slug embedded in
        // a longer file-derived title like "21_sexy_gonna_do_it_song".
        let found = available_slugs
            .iter()
            .find(|s| **s == slug)
            .or_else(|| available_slugs.iter().find(|s| slug.contains(s.as_str()) || s.contains(&slug)));
        if let Some(found) = found {
            matched.push((found.clone(), song));
        }
    }

    println!(
        "matched {}/{} available transcript sets: {:?}",
        matched.len(),
        available_slugs.len(),
        matched.iter().map(|(s, _)| s.clone()).collect::<Vec<_>>()
    );

    if matched.is_empty() {
        eprintln!("no songs matched available bench_out transcripts by title slug");
        std::process::exit(1);
    }

    let mut durations = Vec::new();
    for (slug, song) in &matched {
        let variant_dir = transcripts_dir.join(slug);
        // Several bench variants (e.g. `*_qwen.json`) are re-aligned text
        // with every segment/word timestamp left at 0.0 -- fine for
        // accuracy scoring, useless for timing our render. Prefer a `ctc`
        // variant (matches this project's `align_backend` config option;
        // real per-word timings, confirmed by inspection) and fall back to
        // whatever's first.
        let Some(variant_file) = std::fs::read_dir(&variant_dir)
            .ok()
            .map(|it| it.filter_map(|e| e.ok()).map(|e| e.path()).collect::<Vec<_>>())
            .and_then(|mut paths| {
                paths.sort();
                paths
                    .iter()
                    .find(|p| p.to_string_lossy().contains("_ctc"))
                    .cloned()
                    .or_else(|| paths.first().cloned())
            })
        else {
            eprintln!("[{slug}] no transcript variant file found, skipping");
            continue;
        };

        let transcript_json =
            std::fs::read_to_string(&variant_file).expect("read transcript variant");
        std::fs::write(cache.transcript_path(&song.file_hash), &transcript_json)
            .expect("seed transcript cache");
        // Force a fresh render even if a previous run already cached one.
        let _ = std::fs::remove_file(cache.karaoke_video_path(&song.file_hash));

        print!(
            "[{slug}] rendering \"{}\" ({:.1}s song)... ",
            song.title, song.duration_secs
        );
        use std::io::Write;
        std::io::stdout().flush().ok();

        let start = Instant::now();
        match app_core::ensure_karaoke_video(&song.file_hash, false) {
            Ok(path) => {
                let elapsed = start.elapsed();
                println!("{:.2}s -> {}", elapsed.as_secs_f64(), path.display());
                durations.push(elapsed.as_secs_f64());
            }
            Err(e) => {
                println!("FAILED: {e}");
            }
        }
    }

    if durations.is_empty() {
        eprintln!("no successful renders");
        std::process::exit(1);
    }

    let sum: f64 = durations.iter().sum();
    let min = durations.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = durations.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    println!(
        "\n{} renders -- min {:.2}s, max {:.2}s, avg {:.2}s",
        durations.len(),
        min,
        max,
        sum / durations.len() as f64
    );

    println!("scratch data dir: {}", scratch.display());
}
