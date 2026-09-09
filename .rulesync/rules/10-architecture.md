---
root: false
targets: ['*']
description: 'Shared karaoke core and desktop/self-hosted adapter boundaries'
---

# Architecture

Keep architecture modular. Organize modules by cohesive product semantics, give each module a narrow public boundary, and keep implementation details private. Prefer small composable units and explicit dependencies over monoliths, global registries, service locators, or cross-module reach-through. Introduce directories and abstractions only when real behavior requires them.

Intended dependency direction:

```text
React UI -> typed bridge/client port -> Tauri invoke adapter --+
                                                      +-> app-core application/domain behavior -> infrastructure
React UI -> typed bridge/client port -> HTTP/WS adapter -------+
```

- Domain and application logic in `app-core/` must remain platform-neutral. It must not depend on React, browser globals, Tauri, Axum, transport payloads, or UI state.
- Tauri commands in `client/src-tauri/` and HTTP/WebSocket handlers in `client/src-server/` are thin adapters: authenticate where applicable, parse and validate input, call shared behavior, and map typed output/errors.
- Desktop and self-hosted delivery must use the same karaoke behavior and contracts. Do not fix parity by duplicating logic in adapters.
- Bind concrete adapters only at explicit composition roots. Use narrow traits or typed function boundaries, not global mutable state.
- Keep transport DTOs separate from domain models when serialization, permissions, versioning, or compatibility differ.
- Frontend code must not assume Tauri exists. Native capabilities belong behind typed modules in `client/src/bridge/` with explicit browser/server implementations.
- Prevent dependency cycles and deep imports into another feature's private files. Move genuinely shared frontend behavior into a cohesive shared module.
- Use the `@/` alias for imports rooted at `client/src/`; do not traverse application boundaries with relative paths.
- Public module boundaries should be small and intentional. Callers must not depend on storage, transport, framework, or rendering details.
- Keep shared behavior single-sourced. Reuse an existing module or extract the smallest cohesive shared operation when two paths implement the same contract; do not force unrelated behavior through abstractions merely to remove similar syntax.
- Centralize frontend runtime selection behind typed bridge/client boundaries. Feature clients validate external responses and must not independently branch between Tauri and HTTP.
- Keep route and page components as composition shells. Put feature behavior in cohesive hooks, contexts, atoms, query modules, or providers; split independently meaningful UI into components instead of accumulating render helpers and state machines in pages.
- Reuse shadcn primitives from `client/src/components/ui/`; do not create wrapper layers without repeated product behavior that needs them.
- Keep effects at system edges. Time, randomness, network, storage, microphone/audio devices, subprocesses, and filesystem access need narrow interfaces when they influence behavior.
