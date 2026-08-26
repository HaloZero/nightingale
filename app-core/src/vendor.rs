use std::{
    path::{Path, PathBuf},
    process::Command,
};

use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use tracing::info;
use ts_rs::TS;

use crate::{
    cache::{
        CachePaths, change_app_data_path, migrate_directory_contents, models_dir, nightingale_dir,
        normalized_target_path, same_path, songs_cache_dir, vendor_dir, videos_dir,
    },
    playback::prefetch_one_per_flavor,
    vendor_scripts,
};

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum SetupStep {
    MigrateData,
    ClearVendor,
    Ffmpeg,
    Uv,
    Python,
    Venv,
    Dependencies,
    ExtractScripts,
    Videos,
    Finish,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SetupProgress {
    pub step: SetupStep,
    pub percent: usize,
    pub action: String,
}

pub fn resolve_data_path_input(input: &str) -> Result<PathBuf, String> {
    normalized_target_path(PathBuf::from(input))
}

// ─── Directory Helpers ───────────────────────────────────────────────

pub fn clear_vendor_dir() -> Result<(), String> {
    let dir = vendor_dir();
    if dir.is_dir() {
        std::fs::remove_dir_all(&dir)
            .map_err(|e| format!("Failed to clear vendor directory: {e}"))?;
    }
    Ok(())
}

pub fn ffmpeg_path() -> PathBuf {
    let name = if cfg!(windows) {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    };

    vendor_dir().join(name)
}

/// Font bundled for karaoke-video text rendering (`crate::karaoke_video`).
/// Not part of `is_ready()`/the setup wizard -- casting with burned-in
/// lyrics is an opt-in, niche feature (`ChromecastConfig.karaoke_video`),
/// not core setup, so it's downloaded lazily on first use instead
/// (`ensure_font_downloaded`).
pub fn font_path() -> PathBuf {
    vendor_dir().join("DejaVuSans.ttf")
}

pub fn python_path() -> PathBuf {
    if cfg!(windows) {
        vendor_dir().join("venv").join("Scripts").join("python.exe")
    } else {
        vendor_dir().join("venv").join("bin").join("python")
    }
}

pub fn analyzer_dir() -> PathBuf {
    vendor_dir().join("analyzer")
}

fn uv_path() -> PathBuf {
    let name = if cfg!(windows) { "uv.exe" } else { "uv" };
    vendor_dir().join(name)
}

fn ready_marker() -> PathBuf {
    vendor_dir().join(".ready")
}

pub fn is_ready() -> bool {
    ready_marker().is_file()
        && ffmpeg_path().is_file()
        && python_path().is_file()
        && analyzer_dir().join("analyze.py").is_file()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SetupFolders {
    pub data_path: Option<String>,
    pub cache_paths: Option<CachePaths>,
}

fn normalize_optional_path(path: Option<PathBuf>) -> Result<Option<PathBuf>, String> {
    path.map(normalized_target_path).transpose()
}

fn normalize_cache_paths(paths: CachePaths) -> Result<CachePaths, String> {
    Ok(CachePaths {
        songs: normalize_optional_path(paths.songs)?,
        videos: normalize_optional_path(paths.videos)?,
        models: normalize_optional_path(paths.models)?,
        vendor: normalize_optional_path(paths.vendor)?,
    })
}

fn default_cache_paths_for_data_root() -> CachePaths {
    let root = nightingale_dir();
    CachePaths {
        songs: Some(root.join("cache")),
        videos: Some(root.join("videos")),
        models: Some(root.join("models")),
        vendor: Some(root.join("vendor")),
    }
}

fn migrate_cache_data_to_targets(targets: &CachePaths) -> Result<(), String> {
    let old_songs = songs_cache_dir();
    let old_videos = videos_dir();
    let old_models = models_dir();

    if let Some(target) = targets.songs.as_ref() {
        migrate_directory_contents(&old_songs, target)?;
    }
    if let Some(target) = targets.videos.as_ref() {
        migrate_directory_contents(&old_videos, target)?;
    }
    if let Some(target) = targets.models.as_ref() {
        migrate_directory_contents(&old_models, target)?;
    }

    Ok(())
}

pub fn run_vendor_setup(
    folders: SetupFolders,
    mut on_progress: impl FnMut(SetupProgress) + Send,
    mut on_data_migrated: impl FnMut(&Path) -> Result<(), String>,
) -> Result<(), String> {
    let mut emit = |step: SetupStep, percent: usize, action: String| {
        on_progress(SetupProgress {
            step,
            percent,
            action,
        });
    };

    let mut cleared_vendor = false;
    if let Some(raw_path) = folders
        .data_path
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let target = resolve_data_path_input(raw_path)?;
        let current = nightingale_dir();
        if !same_path(&current, &target) {
            emit(
                SetupStep::ClearVendor,
                6,
                "Clearing vendor folder before migration...".to_string(),
            );
            clear_vendor_dir()?;
            cleared_vendor = true;

            emit(
                SetupStep::MigrateData,
                12,
                "Migrating app data...".to_string(),
            );
            let new_path = change_app_data_path(target)?;
            on_data_migrated(&new_path)?;
            emit(
                SetupStep::MigrateData,
                18,
                format!("Data migrated to {}", new_path.display()),
            );
        }
    }

    emit(
        SetupStep::MigrateData,
        20,
        "Moving cache data to selected folders...".to_string(),
    );

    let separate_targets = folders.cache_paths.map(normalize_cache_paths).transpose()?;
    let targets = separate_targets
        .clone()
        .unwrap_or_else(default_cache_paths_for_data_root);
    let old_songs_cache = songs_cache_dir();
    migrate_cache_data_to_targets(&targets)?;
    if let Some(new_songs_cache) = targets.songs.as_ref() {
        crate::library_db::rebase_song_album_art_cache_paths(&old_songs_cache, new_songs_cache)?;
    }

    let mut cfg = crate::config::AppConfig::load();
    cfg.cache_paths = separate_targets;
    cfg.save();

    if !cleared_vendor {
        emit(
            SetupStep::ClearVendor,
            14,
            "Clearing vendor folder...".to_string(),
        );
        clear_vendor_dir()?;
    }

    emit(SetupStep::Ffmpeg, 24, "Downloading ffmpeg...".to_string());
    step_download_ffmpeg()?;

    emit(SetupStep::Uv, 34, "Downloading uv...".to_string());
    step_download_uv()?;

    emit(
        SetupStep::Python,
        46,
        "Installing python3.10 via uv...".to_string(),
    );
    step_install_python()?;

    emit(SetupStep::Venv, 58, "Setting up .venv...".to_string());
    step_create_venv()?;

    emit(
        SetupStep::Dependencies,
        70,
        "Installing python dependencies...".to_string(),
    );
    step_install_packages()?;

    emit(
        SetupStep::ExtractScripts,
        80,
        "Extracting analyzer scripts...".to_string(),
    );
    step_extract_scripts()?;

    emit(
        SetupStep::Videos,
        90,
        "Pre-downloading video backgrounds...".to_string(),
    );
    prefetch_one_per_flavor(|detail| {
        emit(SetupStep::Videos, 90, detail.to_string());
    });

    mark_ready()?;
    emit(SetupStep::Finish, 100, "Done".to_string());

    Ok(())
}

// ─── Download helpers ───────────────────────────────────────────────

fn download_to_file(url: &str, dest: &std::path::Path) -> Result<(), String> {
    let resp = ureq::get(url).call().map_err(|e| e.to_string())?;
    let mut body = resp.into_body();
    let mut reader = body.as_reader();
    let mut file = std::fs::File::create(dest).map_err(|e| e.to_string())?;
    std::io::copy(&mut reader, &mut file).map_err(|e| e.to_string())?;
    Ok(())
}

fn extract_archive(archive: &std::path::Path, dest_dir: &std::path::Path) -> Result<(), String> {
    let name = archive.to_string_lossy();

    let output = if name.ends_with(".tar.xz") {
        silent_command("tar")
            .arg("-xmJf")
            .arg(archive)
            .arg("-C")
            .arg(dest_dir)
            .output()
    } else if name.ends_with(".tar.gz") {
        silent_command("tar")
            .arg("-xmzf")
            .arg(archive)
            .arg("-C")
            .arg(dest_dir)
            .output()
    } else if name.ends_with(".zip") {
        #[cfg(windows)]
        {
            silent_command("tar")
                .arg("-xmf")
                .arg(archive)
                .arg("-C")
                .arg(dest_dir)
                .output()
        }
        #[cfg(not(windows))]
        {
            silent_command("unzip")
                .arg("-o")
                .arg(archive)
                .arg("-d")
                .arg(dest_dir)
                .output()
        }
    } else {
        return Err(format!("Unknown archive format: {name}"));
    };

    let output = output.map_err(|e| format!("Failed to run extraction command: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Extraction failed: {stderr}"));
    }
    Ok(())
}

fn find_file_in(dir: &std::path::Path, name: &str) -> Option<PathBuf> {
    walkdir::WalkDir::new(dir)
        .into_iter()
        .flatten()
        .find(|e| e.file_type().is_file() && e.file_name().to_string_lossy() == name)
        .map(|e| e.into_path())
}

fn mark_executable(_path: &std::path::Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(_path, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("Failed to set permissions: {e}"))?;
    }
    Ok(())
}

// ─── Other Helpers ───────────────────────────────────────────────────

pub fn silent_command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

// ─── Step 1: Download ffmpeg ─────────────────────────────────────────

fn ffmpeg_download_url() -> Result<&'static str, String> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => {
            Ok("https://johnvansickle.com/ffmpeg/releases/ffmpeg-release-amd64-static.tar.xz")
        }
        ("linux", "aarch64") => {
            Ok("https://johnvansickle.com/ffmpeg/releases/ffmpeg-release-arm64-static.tar.xz")
        }
        ("macos", "aarch64") => Ok("https://evermeet.cx/ffmpeg/ffmpeg-8.1.zip"),
        ("macos", "x86_64") => Ok("https://evermeet.cx/ffmpeg/ffmpeg-8.1.zip"),
        ("windows", "x86_64") => Ok(
            "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip",
        ),
        (os, arch) => Err(format!("Unsupported platform for ffmpeg: {os}-{arch}")),
    }
}

pub fn step_download_ffmpeg() -> Result<(), String> {
    let dest = ffmpeg_path();
    if dest.is_file() {
        return Ok(());
    }

    let url = ffmpeg_download_url()?;

    let tmp_dir = vendor_dir().join("_tmp_ffmpeg");
    let _ = std::fs::create_dir_all(&tmp_dir);

    let ext = if url.ends_with(".tar.xz") {
        "tar.xz"
    } else {
        "zip"
    };
    let archive = tmp_dir.join(format!("ffmpeg.{ext}"));

    let result: Result<(), String> = (|| {
        download_to_file(url, &archive)?;

        extract_archive(&archive, &tmp_dir)?;

        let binary_name = if cfg!(windows) {
            "ffmpeg.exe"
        } else {
            "ffmpeg"
        };
        let found = find_file_in(&tmp_dir, binary_name)
            .ok_or_else(|| format!("Could not find {binary_name} in downloaded archive"))?;

        std::fs::copy(&found, &dest).map_err(|e| format!("Failed to copy ffmpeg: {e}"))?;
        mark_executable(&dest)?;
        Ok(())
    })();

    let _ = std::fs::remove_dir_all(&tmp_dir);
    result?;

    Ok(())
}

// ─── Karaoke video font (lazy, not a wizard step) ───────────────────

// DejaVu ships pre-built TTFs only as a GitHub Release asset (the repo's
// git tree holds font *sources*, not binaries), so this is a zip like
// ffmpeg's/uv's downloads, not a raw single-file fetch.
const FONT_DOWNLOAD_URL: &str = "https://github.com/dejavu-fonts/dejavu-fonts/releases/download/version_2_37/dejavu-sans-ttf-2.37.zip";

/// Downloads the bundled font on first use. DejaVu Sans specifically:
/// Bitstream-Vera-derived license explicitly permits embedding/
/// redistribution, and it's the de facto default across the ffmpeg/
/// subtitle tooling ecosystem. Latin-script only -- no CJK/RTL coverage.
pub fn ensure_font_downloaded() -> Result<PathBuf, String> {
    let dest = font_path();
    if dest.is_file() {
        return Ok(dest);
    }

    let tmp_dir = vendor_dir().join("_tmp_font");
    let _ = std::fs::create_dir_all(&tmp_dir);
    let archive = tmp_dir.join("dejavu.zip");

    let result: Result<(), String> = (|| {
        download_to_file(FONT_DOWNLOAD_URL, &archive)?;
        extract_archive(&archive, &tmp_dir)?;
        let found = find_file_in(&tmp_dir, "DejaVuSans.ttf")
            .ok_or_else(|| "Could not find DejaVuSans.ttf in downloaded archive".to_string())?;
        std::fs::copy(&found, &dest).map_err(|e| format!("Failed to copy font: {e}"))?;
        Ok(())
    })();

    let _ = std::fs::remove_dir_all(&tmp_dir);
    result.map_err(|e| format!("Failed to download karaoke-video font: {e}"))?;

    Ok(dest)
}

// ─── Step 2: Download uv ────────────────────────────────────────────

fn uv_download_url() -> Result<&'static str, String> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Ok(
            "https://github.com/astral-sh/uv/releases/latest/download/uv-x86_64-unknown-linux-gnu.tar.gz",
        ),
        ("linux", "aarch64") => Ok(
            "https://github.com/astral-sh/uv/releases/latest/download/uv-aarch64-unknown-linux-gnu.tar.gz",
        ),
        ("macos", "aarch64") => Ok(
            "https://github.com/astral-sh/uv/releases/latest/download/uv-aarch64-apple-darwin.tar.gz",
        ),
        ("macos", "x86_64") => Ok(
            "https://github.com/astral-sh/uv/releases/latest/download/uv-x86_64-apple-darwin.tar.gz",
        ),
        ("windows", "x86_64") => Ok(
            "https://github.com/astral-sh/uv/releases/latest/download/uv-x86_64-pc-windows-msvc.zip",
        ),
        (os, arch) => Err(format!("Unsupported platform for uv: {os}-{arch}")),
    }
}

pub fn step_download_uv() -> Result<(), String> {
    let dest = uv_path();
    if dest.is_file() {
        return Ok(());
    }

    let url = uv_download_url()?;

    let tmp_dir = vendor_dir().join("_tmp_uv");
    let _ = std::fs::create_dir_all(&tmp_dir);

    let ext = if url.ends_with(".zip") {
        "zip"
    } else {
        "tar.gz"
    };
    let archive = tmp_dir.join(format!("uv.{ext}"));

    let result: Result<(), String> = (|| {
        download_to_file(url, &archive)?;
        extract_archive(&archive, &tmp_dir)?;

        let binary_name = if cfg!(windows) { "uv.exe" } else { "uv" };
        let found = find_file_in(&tmp_dir, binary_name)
            .ok_or_else(|| format!("Could not find {binary_name} in downloaded archive"))?;

        std::fs::copy(&found, &dest).map_err(|e| format!("Failed to copy uv: {e}"))?;
        mark_executable(&dest)?;
        Ok(())
    })();

    let _ = std::fs::remove_dir_all(&tmp_dir);
    result?;

    Ok(())
}

// ─── Step 3: Install Python via uv ──────────────────────────────────

pub fn step_install_python() -> Result<(), String> {
    let python_dir = vendor_dir().join("python");
    if python_dir.is_dir() && has_python_in(&python_dir) {
        return Ok(());
    }

    let output = silent_command(uv_path())
        .args(["python", "install", "3.10", "--install-dir"])
        .arg(&python_dir)
        .output()
        .map_err(|e| format!("Failed to run uv: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("uv python install failed: {stderr}"));
    }

    Ok(())
}

fn has_python_in(dir: &PathBuf) -> bool {
    if !dir.is_dir() {
        return false;
    }
    let target = if cfg!(windows) {
        "python.exe"
    } else {
        "python3.10"
    };
    for entry in walkdir::WalkDir::new(dir)
        .max_depth(5)
        .into_iter()
        .flatten()
    {
        if entry.file_type().is_file() && entry.file_name().to_string_lossy() == target {
            return true;
        }
    }
    false
}

// ─── Step 4: Create venv ─────────────────────────────────────────────

fn find_installed_python() -> Option<PathBuf> {
    let python_dir = vendor_dir().join("python");
    let target = if cfg!(windows) {
        "python.exe"
    } else {
        "python3.10"
    };
    for entry in walkdir::WalkDir::new(&python_dir)
        .max_depth(5)
        .into_iter()
        .flatten()
    {
        if entry.file_type().is_file() && entry.file_name().to_string_lossy() == target {
            return Some(entry.into_path());
        }
    }
    None
}

pub fn step_create_venv() -> Result<(), String> {
    let venv_dir = vendor_dir().join("venv");
    if python_path().is_file() {
        return Ok(());
    }

    let installed_python = find_installed_python()
        .ok_or("Could not find installed Python — run python install first")?;

    let output = silent_command(uv_path())
        .args(["venv"])
        .arg(&venv_dir)
        .arg("--python")
        .arg(&installed_python)
        .output()
        .map_err(|e| format!("Failed to run uv venv: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("uv venv failed: {stderr}"));
    }

    Ok(())
}

// ─── Step 5: Install packages ────────────────────────────────────────

struct GpuInfo {
    device: &'static str,
    torch_index: &'static str,
    legacy_torch: bool,
}

fn detect_gpu() -> GpuInfo {
    #[cfg(target_os = "macos")]
    {
        if cfg!(target_arch = "x86_64") {
            info!("[vendor] GPU detection: Intel Mac (CPU-only, torch < 2.3)");
            return GpuInfo {
                device: "cpu",
                torch_index: "https://download.pytorch.org/whl/cpu",
                legacy_torch: true,
            };
        }
        return GpuInfo {
            device: "mps",
            torch_index: "https://download.pytorch.org/whl/cpu",
            legacy_torch: false,
        };
    }

    #[cfg(not(target_os = "macos"))]
    {
        match nvidia_smi_path() {
            Some(smi) => {
                let cuda_index = query_cuda_index(&smi);
                info!("[vendor] GPU detection: CUDA (index {cuda_index})");
                GpuInfo {
                    device: "cuda",
                    torch_index: cuda_index,
                    legacy_torch: false,
                }
            }
            None => {
                info!("[vendor] GPU detection: CPU (nvidia-smi not found)");
                GpuInfo {
                    device: "cpu",
                    torch_index: "https://download.pytorch.org/whl/cpu",
                    legacy_torch: false,
                }
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn nvidia_smi_path() -> Option<&'static str> {
    let ok = silent_command("nvidia-smi")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success());

    if ok {
        info!("[vendor] nvidia-smi found on PATH");
        Some("nvidia-smi")
    } else {
        info!("[vendor] nvidia-smi not found on PATH");
        None
    }
}

#[cfg(not(target_os = "macos"))]
fn query_cuda_index(nvidia_smi: &str) -> &'static str {
    let output = silent_command(nvidia_smi)
        .args(["--query-gpu=compute_cap", "--format=csv,noheader"])
        .output();

    let major = output.ok().filter(|o| o.status.success()).and_then(|o| {
        let text = String::from_utf8_lossy(&o.stdout).trim().to_string();
        info!("[vendor] GPU compute capability: {text}");
        text.split('.').next().and_then(|m| m.parse::<u32>().ok())
    });

    match major {
        Some(v) if v >= 10 => "https://download.pytorch.org/whl/cu128",
        Some(_) => "https://download.pytorch.org/whl/cu126",
        None => {
            info!("[vendor] Could not query compute capability, falling back to cu126");
            "https://download.pytorch.org/whl/cu126"
        }
    }
}

pub fn step_install_packages() -> Result<(), String> {
    let gpu = detect_gpu();

    let uv = uv_path();
    let py = python_path();
    let py_str = py.to_string_lossy().to_string();
    let index = gpu.torch_index;

    let (audio_sep_pkg, whisperx_pkg) = if gpu.legacy_torch {
        ("audio-separator>=0.24,<0.25", "whisperx>=3.3.0,<3.3.4")
    } else if gpu.device == "cuda" {
        ("audio-separator[gpu]>=0.25", "whisperx>=3.3.0")
    } else {
        ("audio-separator>=0.25", "whisperx>=3.3.0")
    };

    let cython_out = silent_command(&uv)
        .args(["pip", "install", "cython", "setuptools", "--python"])
        .arg(&py)
        .output()
        .map_err(|e| format!("Failed to install build deps: {e}"))?;
    if !cython_out.status.success() {
        let stderr = String::from_utf8_lossy(&cython_out.stderr);
        return Err(format!("Build deps install failed: {stderr}"));
    }

    let mut pkg_args: Vec<&str> = vec![
        "pip",
        "install",
        "demucs>=4.0.0",
        whisperx_pkg,
        "soundfile",
        "huggingface_hub>=0.27.0",
        audio_sep_pkg,
        "onnx-asr>=0.5.0",
        "onnxruntime>=1.17",
        "fugashi[unidic-lite]>=1.3",
        "pykakasi>=2.3",
        "jieba>=0.42",
        "pypinyin>=0.50",
        "ToJyutping>=3.0",
        "hangul-romanize>=0.1.0",
        // Tokenizers the Qwen3 forced aligner uses internally for ja/ko.
        "nagisa>=0.2.11",
        "soynlp>=0.0.493",
    ];

    if gpu.legacy_torch {
        pkg_args.push("torch<2.3");
        pkg_args.push("torchaudio<2.3");
    }

    // Apple Silicon only: gives Whisper a real GPU path via Metal instead of the
    // CPU-only fallback WhisperX's CTranslate2 backend is stuck with on `mps`.
    //
    // Installed from muaz978/mlx-examples#add-whisper-beam-search (upstream PR
    // ml-explore/mlx-examples#1429, unmerged as of 2026-08-13) rather than the
    // PyPI release: plain mlx-whisper raises NotImplementedError for beam_size,
    // see whisper_mlx.py. Pinned to a commit for reproducibility.
    if gpu.device == "mps" {
        pkg_args.push(concat!(
            "mlx-whisper @ git+https://github.com/muaz978/mlx-examples",
            "@bfdc543b3a0e85bad226f7fff637617bd0aa3649#subdirectory=whisper",
        ));
    }

    pkg_args.push("--python");
    pkg_args.push(&py_str);

    let output = silent_command(&uv)
        .args(&pkg_args)
        .output()
        .map_err(|e| format!("Failed to run uv pip install: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Package install failed: {stderr}"));
    }

    if gpu.device == "cuda" {
        let torch_args: Vec<&str> = vec![
            "pip",
            "install",
            "--reinstall-package",
            "torch",
            "--reinstall-package",
            "torchaudio",
            "--reinstall-package",
            "torchvision",
            "torch==2.10.0",
            "torchaudio==2.10.0",
            "torchvision==0.25.0",
            "--python",
            &py_str,
            "--index-url",
            index,
        ];

        let output = silent_command(&uv)
            .args(&torch_args)
            .output()
            .map_err(|e| format!("Failed to install CUDA PyTorch: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("CUDA PyTorch install failed: {stderr}"));
        }

        let nemo_args: Vec<&str> = vec![
            "pip",
            "install",
            "nemo_toolkit[asr]>=2.0.0",
            "--python",
            &py_str,
        ];

        let output = silent_command(&uv)
            .args(&nemo_args)
            .output()
            .map_err(|e| format!("Failed to install NeMo: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("NeMo install failed: {stderr}"));
        }
    }

    // Qwen3-ForcedAligner (experimental align backend) needs the Qwen3-ASR
    // integration, which landed in transformers main (PR #43838) but is not in
    // any tagged release yet. Install it from the merge commit over whatever
    // whisperx pulled in; this stays a no-upper-bound override so whisperx's
    // own transformers usage keeps working. Pinned for reproducibility.
    //
    // This MUST run last: on CUDA, `nemo_toolkit[asr]` (installed above) pins
    // transformers back to a tagged release that doesn't recognize the
    // `qwen3_asr` model type, so it has to be re-applied after NeMo to win.
    let transformers_git = concat!(
        "transformers @ git+https://github.com/huggingface/transformers",
        "@967203924487e8e9f64a2d825fc4e1bdbec3f518",
    );
    let transformers_args: Vec<&str> = vec![
        "pip",
        "install",
        "--reinstall-package",
        "transformers",
        transformers_git,
        "--python",
        &py_str,
    ];

    let output = silent_command(&uv)
        .args(&transformers_args)
        .output()
        .map_err(|e| format!("Failed to install Qwen-capable transformers: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("transformers (Qwen3-ASR) install failed: {stderr}"));
    }

    Ok(())
}

// ─── Step 6: Extract analyzer scripts ────────────────────────────────

pub fn step_extract_scripts() -> Result<(), String> {
    vendor_scripts::write_scripts(&analyzer_dir())
        .map_err(|e| format!("Failed to write scripts: {e}"))?;
    Ok(())
}

/// Refresh the embedded analyzer scripts on top of an already-set-up vendor dir.
/// No-op when setup hasn't completed yet — initial extraction is handled by
/// `step_extract_scripts` during the setup flow.
pub fn refresh_analyzer_scripts_if_ready() -> Result<(), String> {
    if !is_ready() {
        return Ok(());
    }

    vendor_scripts::write_scripts(&analyzer_dir())
        .map_err(|e| format!("Failed to refresh analyzer scripts: {e}"))
}

pub fn mark_ready() -> Result<(), String> {
    std::fs::write(ready_marker(), "ok").map_err(|e| format!("Failed to mark ready: {e}"))
}
