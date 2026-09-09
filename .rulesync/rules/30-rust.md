---
root: false
targets: ['*']
description: 'Strict Rust correctness, async, API, and adapter rules'
globs:
  - '**/*.rs'
  - '**/Cargo.toml'
---

# Rust

- Keep workspace lint policy inherited by every crate. `unsafe` is forbidden unless policy is deliberately revisited with an isolated safety contract and explicit approval.
- Prefer typed errors with useful context. Do not panic, unwrap, or discard errors on user input, network, storage, media, filesystem, analyzer, or other recoverable runtime paths. A process entry point may use `expect` only for an unrecoverable startup invariant and must provide an actionable message.
- Do not block an async executor with synchronous filesystem, network, database, subprocess, sleep, audio, or CPU-heavy work. Use async APIs or an explicit blocking/worker boundary.
- Tauri commands and server handlers remain thin. Parse and validate transport input, enforce authorization where applicable, invoke `app-core`, and translate typed errors without exposing internal details.
- Domain code in `app-core` must not depend on Tauri, Axum, browser-facing transport details, or concrete UI state. Depend on narrow traits owned by consumers.
- Keep public APIs minimal. Prefer borrowing over cloning by default, but choose clarity and correct ownership over micro-optimization.
- Serialization, caches, databases, profiles, config, lyric formats, and persisted types are compatibility contracts. Use explicit representations, reject invalid values where appropriate, and review migration/backward compatibility before changing them.
- Avoid ambient global mutable state. Shared mutable state requires a clear concurrency model and must not hold a synchronous lock across `.await`.
- Secrets, tokens, private paths, lyrics, microphone data, and user media metadata must not appear in logs or error responses. Use structured logging and deliberate redaction.

Do not add automated Rust tests. Validate through rustfmt, `cargo check`, Clippy with warnings denied, dependency auditing, and focused manual behavior checks. Run `cargo deny check` after dependency or supply-chain policy changes.
