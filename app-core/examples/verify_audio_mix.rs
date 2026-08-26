//! Verifies a karaoke video render (instrumental+guide-vocal mix, fps,
//! background) in a fully isolated scratch data dir -- copies one real,
//! already-analyzed song's audio + cache (transcript, real stems) and a
//! couple of real nature backgrounds in as read-only source copies, so
//! this never touches the user's live `~/.nightingale` dir or its
//! already-running server.
//!
//! Run: `cargo run -p app-core --release --example verify_audio_mix -- <file_hash> <audio_src_path>`
//! Defaults to the standing sample song ("Adam's Song" -- Blink 182) if no
//! args are given.

use std::path::PathBuf;

const REAL_DATA_DIR: &str = "/Users/rohandhaimade/.nightingale";
const DEFAULT_AUDIO_SRC: &str =
    "/Users/rohandhaimade/Library/CloudStorage/Dropbox/iTunes/iTunes Media/Music/Blink 182/Enema of the State/Adam's Song.m4a";
const DEFAULT_FILE_HASH: &str = "e9eece443c3ec8a04098cd0a66f22734";

fn main() {
    let mut args = std::env::args().skip(1);
    let file_hash = args.next().unwrap_or_else(|| DEFAULT_FILE_HASH.to_string());
    let audio_src = args.next().unwrap_or_else(|| DEFAULT_AUDIO_SRC.to_string());
    let audio_ext = PathBuf::from(&audio_src)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("mp3")
        .to_string();

    // REUSE_SCRATCH_DIR points at a prior run's scratch dir (e.g. one that
    // already has a built nature-reel pool) to avoid repeating the ~16min
    // reel build just to re-render with an unrelated change.
    let scratch = match std::env::var("REUSE_SCRATCH_DIR") {
        Ok(dir) => PathBuf::from(dir),
        Err(_) => std::env::temp_dir().join(format!(
            "nightingale-verify-audio-mix-{}",
            std::process::id()
        )),
    };
    let songs_dir = scratch.join("songs");
    std::fs::create_dir_all(&songs_dir).expect("create scratch songs dir");
    std::fs::create_dir_all(scratch.join("cache")).expect("create scratch cache dir");
    let nature_dir = scratch.join("videos").join("nature");
    std::fs::create_dir_all(&nature_dir).expect("create scratch nature dir");

    // Copy the real audio file byte-for-byte -- content-addressed hashing
    // means the scratch scan produces the exact same file_hash, letting us
    // reuse the real cache files copied below directly.
    std::fs::copy(&audio_src, songs_dir.join(format!("song.{audio_ext}")))
        .expect("copy real audio");

    // Discover the real key/tempo-suffixed stem filenames instead of
    // hardcoding them -- differs per song (e.g. `_Fm_1.0` vs `_Am_1.0`).
    let real_cache = PathBuf::from(REAL_DATA_DIR).join("cache");
    let mut stem_names: Vec<String> = std::fs::read_dir(&real_cache)
        .expect("read real cache dir")
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| name.starts_with(&format!("{file_hash}_")))
        .collect();
    stem_names.push(format!("{file_hash}_transcript.json"));
    stem_names.retain(|name| {
        name.contains("_instrumental_") || name.contains("_vocals_") || name.ends_with("_transcript.json")
    });

    for name in &stem_names {
        std::fs::copy(real_cache.join(name), scratch.join("cache").join(name))
            .unwrap_or_else(|e| panic!("copy real cache file {name}: {e}"));
    }

    // Enough raw clips for `build_background_reels` to have real material to
    // work with (set BUILD_REELS=1 below); a handful is enough for the
    // plain no-reel fallback path.
    let with_reels = std::env::var("BUILD_REELS").is_ok();
    let nature_clip_count = if with_reels { 40 } else { 3 };
    let real_nature = PathBuf::from(REAL_DATA_DIR).join("videos").join("nature");
    for entry in std::fs::read_dir(&real_nature)
        .expect("read real nature dir")
        .filter_map(|e| e.ok())
        .take(nature_clip_count)
    {
        std::fs::copy(entry.path(), nature_dir.join(entry.file_name()))
            .expect("copy real nature video");
    }

    unsafe { std::env::set_var("NIGHTINGALE_DATA_PATH", &scratch) };
    app_core::startup().expect("app_core startup");

    let vendor_dir = scratch.join("vendor");
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
    let vendored_ffmpeg = vendor_dir.join("ffmpeg");
    std::fs::copy(&system_ffmpeg, &vendored_ffmpeg).expect("copy ffmpeg into vendor dir");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&vendored_ffmpeg, std::fs::Permissions::from_mode(0o755))
            .expect("chmod ffmpeg");
    }

    let mut config = app_core::AppConfig::load();
    config.library_source = Some(app_core::LibrarySource::Folder { path: songs_dir.clone() });
    config.save();
    app_core::start_scan();

    println!("scanning {} ...", songs_dir.display());
    let mut found = false;
    for _ in 0..30 {
        std::thread::sleep(std::time::Duration::from_millis(500));
        let store = app_core::SongsStore::load(&app_core::LoadSongsParams {
            search: None,
            filters: app_core::LibraryMenuFilters::default(),
            skip: 0,
            take: 10,
        });
        if store.processed.iter().any(|s| s.file_hash == file_hash) {
            found = true;
            break;
        }
    }
    if !found {
        eprintln!("scan never produced the expected file_hash -- audio file copy may differ from the real one");
        std::process::exit(1);
    }
    println!("scan confirmed matching file_hash: {file_hash}");

    if with_reels {
        println!("building nature reels...");
        let reel_start = std::time::Instant::now();
        app_core::build_background_reels("nature", |msg| println!("  {msg}"));
        println!("reels built in {:.1}s", reel_start.elapsed().as_secs_f64());
    }

    let start = std::time::Instant::now();
    match app_core::ensure_karaoke_video(&file_hash, true) {
        Ok(path) => println!(
            "rendered in {:.1}s -> {}",
            start.elapsed().as_secs_f64(),
            path.display()
        ),
        Err(e) => {
            eprintln!("FAILED: {e}");
            std::process::exit(1);
        }
    }
}
