//! At-rest obfuscation for credentials we have to keep on disk (the Jellyfin
//! access token today, anything similar tomorrow).
//!
//! Threat model: protect the secret against passive disk reads — config
//! backups, accidental sharing of `config.json`, log scrapers. We do NOT defend
//! against an attacker with code execution as the same user; once they can run
//! anything they can also read the key.
//!
//! Implementation: ChaCha20-Poly1305 AEAD with a 256-bit key derived from
//! either the OS machine identifier (`/etc/machine-id`, `IOPlatformUUID`,
//! `MachineGuid`) or — when none is available — a key file persisted at
//! `nightingale_dir()/.secret_seed` with `0600` perms. The key is derived via
//! Blake3's KDF mode with a fixed context string.
//!
//! On-disk envelope (a single string): `enc:v1:<base64(nonce(12) || ct(...))>`.
//! Anything that doesn't carry the `enc:v1:` prefix is treated as legacy
//! plaintext on decrypt and re-encrypted on the next save — that's the
//! migration story.

use std::path::PathBuf;
use std::sync::OnceLock;

use base64::Engine;
use base64::engine::general_purpose::STANDARD_NO_PAD as B64;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};

use crate::cache::default_nightingale_dir;

const KDF_CONTEXT: &str = "nightingale 2026-05 secret-v1";
const ENVELOPE_PREFIX: &str = "enc:v1:";

/// Encrypt `plain` into the on-disk envelope format. Empty strings round-trip
/// to empty strings (no envelope wrap — there's nothing to protect).
pub(crate) fn encrypt_string(plain: &str) -> String {
    if plain.is_empty() {
        return String::new();
    }
    let key = derive_key();
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&key));
    let nonce_bytes: [u8; 12] = rand::random();
    let nonce = Nonce::from_slice(&nonce_bytes);
    let Ok(ciphertext) = cipher.encrypt(nonce, plain.as_bytes()) else {
        return plain.to_string();
    };
    let mut payload = Vec::with_capacity(12 + ciphertext.len());
    payload.extend_from_slice(&nonce_bytes);
    payload.extend_from_slice(&ciphertext);
    format!("{ENVELOPE_PREFIX}{}", B64.encode(payload))
}

/// Decrypt an envelope. Strings without the `enc:v1:` prefix pass through
/// unchanged so we can migrate legacy plaintext rows transparently.
pub(crate) fn decrypt_string(value: &str) -> String {
    let Some(b64) = value.strip_prefix(ENVELOPE_PREFIX) else {
        return value.to_string();
    };
    let Ok(payload) = B64.decode(b64) else {
        return String::new();
    };
    if payload.len() < 12 {
        return String::new();
    }
    let (nonce_bytes, ciphertext) = payload.split_at(12);
    let key = derive_key();
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&key));
    match cipher.decrypt(Nonce::from_slice(nonce_bytes), ciphertext) {
        Ok(plain) => String::from_utf8(plain).unwrap_or_default(),
        Err(_) => String::new(),
    }
}

/// True if `value` is already wrapped — used to skip the round-trip when a
/// caller wants to know "did this load come back encrypted?".
pub(crate) fn is_encrypted(value: &str) -> bool {
    value.starts_with(ENVELOPE_PREFIX)
}

fn derive_key() -> [u8; 32] {
    static CACHED: OnceLock<[u8; 32]> = OnceLock::new();
    *CACHED.get_or_init(|| {
        let seed = machine_seed();
        blake3::derive_key(KDF_CONTEXT, seed.as_bytes())
    })
}

fn machine_seed() -> String {
    if let Some(id) = read_machine_id() {
        return id;
    }
    read_or_create_keyfile().unwrap_or_else(|_| "nightingale-fallback-seed".to_string())
}

#[cfg(target_os = "linux")]
fn read_machine_id() -> Option<String> {
    std::fs::read_to_string("/etc/machine-id")
        .or_else(|_| std::fs::read_to_string("/var/lib/dbus/machine-id"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(target_os = "macos")]
fn read_machine_id() -> Option<String> {
    use std::process::Command;
    let out = Command::new("/usr/sbin/ioreg")
        .args(["-rd1", "-c", "IOPlatformExpertDevice"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    for line in stdout.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("\"IOPlatformUUID\" = \"") {
            return rest.strip_suffix('"').map(|s| s.to_string());
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn read_machine_id() -> Option<String> {
    use std::process::Command;
    let out = Command::new("reg")
        .args([
            "query",
            "HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Cryptography",
            "/v",
            "MachineGuid",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    for line in stdout.lines() {
        let trimmed = line.trim();
        if let Some(idx) = trimmed.find("REG_SZ") {
            let value = trimmed[idx + "REG_SZ".len()..].trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn read_machine_id() -> Option<String> {
    None
}

fn keyfile_path() -> PathBuf {
    default_nightingale_dir().join(".secret_seed")
}

fn read_or_create_keyfile() -> std::io::Result<String> {
    let path = keyfile_path();
    if let Ok(existing) = std::fs::read_to_string(&path) {
        let trimmed = existing.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut bytes = [0u8; 32];
    bytes.iter_mut().for_each(|b| *b = rand::random::<u8>());
    let seed = B64.encode(bytes);
    std::fs::write(&path, &seed)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(seed)
}
