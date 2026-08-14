# Plan

- [x] Add a pinned, hermetic Nix flake that builds `sctui` from `Cargo.lock`. (2026-08-14 04:49 PM EDT)
  - Curiosity poke: identify every native library required by the audio and TLS crates without leaking host dependencies.
- [x] Verify `nix build` succeeds and installs the executable at `result/bin/sctui`. (2026-08-14 04:49 PM EDT)
  - Curiosity poke: confirm the package output itself, not only Cargo's target directory.
- [x] Record file purposes with `dirtree` and commit the passing unit of work. (2026-08-14 04:49 PM EDT)
  - Curiosity poke: keep Peter's untracked `AGENTS.md` outside the commit.
