//! Manual verification tool for `download_all_pixabay_videos`: runs it for
//! real against the Pixabay API and reports how many videos landed and how
//! long it took, since that cost was explicitly flagged as unverified in
//! the plan for this feature.
//!
//! Run: `PIXABAY_API_KEY=... cargo run -p app-core --release --example pixabay_bulk_download -- <flavor>`

use std::time::Instant;

fn main() {
    let flavor = std::env::args().nth(1).unwrap_or_else(|| "nature".to_string());

    let scratch = std::env::temp_dir().join(format!(
        "nightingale-pixabay-bulk-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&scratch).expect("create scratch data dir");
    unsafe { std::env::set_var("NIGHTINGALE_DATA_PATH", &scratch) };

    println!("downloading all '{flavor}' videos into {}", scratch.display());
    let start = Instant::now();
    app_core::download_all_pixabay_videos(&flavor, |msg| println!("{msg}"));
    let elapsed = start.elapsed();

    let dir = scratch.join("videos").join(&flavor);
    let count = std::fs::read_dir(&dir)
        .map(|it| it.filter_map(|e| e.ok()).count())
        .unwrap_or(0);
    let bytes: u64 = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum();

    println!(
        "\ndone in {:.1}s -- {count} files, {:.1} MB, dir: {}",
        elapsed.as_secs_f64(),
        bytes as f64 / 1_000_000.0,
        dir.display()
    );
}
