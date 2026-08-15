# Plan

- [x] Add a pinned, hermetic Nix flake that builds `sctui` from `Cargo.lock`. (2026-08-14 04:49 PM EDT)
  - Curiosity poke: identify every native library required by the audio and TLS crates without leaking host dependencies.
- [x] Verify `nix build` succeeds and installs the executable at `result/bin/sctui`. (2026-08-14 04:49 PM EDT)
  - Curiosity poke: confirm the package output itself, not only Cargo's target directory.
- [x] Record file purposes with `dirtree` and commit the passing unit of work. (2026-08-14 04:49 PM EDT)
  - Curiosity poke: keep Peter's untracked `AGENTS.md` outside the commit.
- [x] Research a credentials fallback compatible with the Go fork at `../sctui` when SoundCloud OAuth environment variables are absent. (2026-08-14 10:35 PM EDT)
  - Curiosity poke: avoid accessing or logging live Firefox secrets; assess profile locking, schema drift, OS portability, and token expiry.
  - Finding: the Go fork queries Firefox `cookies.sqlite` for SoundCloud's `oauth_token`, then sends it as `Authorization: OAuth …` without a refresh token.
  - Finding: Rust needs a non-refreshable browser-session credential mode and centralized `OAuth` header construction; its current unconditional refresh thread and repeated `bearer_auth` calls are incompatible.
  - Finding: two local Firefox cookie databases were inspected without selecting secret values; neither currently contains a SoundCloud cookie.
- [x] Fork `Illogicalll/sctui` into Peter's GitHub account and configure this checkout with distinct fork and upstream remotes. (2026-08-15 02:23 PM EDT)
  - Curiosity poke: detect an existing fork first, preserve fetch access to upstream, and do not disturb the passing local commit.
  - Result: `pmarreck/sctui-rust` is the fork, `origin` points to it, and `upstream` retains `Illogicalll/sctui`.
