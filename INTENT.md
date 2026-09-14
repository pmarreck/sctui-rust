# Intent

This fork gives Peter an interactive SoundCloud player in the terminal, including embedded terminals such as Herdr. It retains the upstream project's library browsing, search, playback, and visualizations.

Peter's requested outcomes are Firefox-session authentication without mandatory environment variables, consistent keyboard and mouse controls across views, discoverable shortcuts, shuffle, and readable playback errors with automatic recovery to a playable track.

Optional graphics must degrade gracefully when terminal capabilities are unavailable. Warnings should explain the limitation without repeatedly interrupting normal use. Playback restrictions should reflect evidence from SoundCloud; an unexplained HTTP response must not be presented as proof of DRM. Circumventing access controls and downloading tracks for offline use are outside this work's scope.

Builds use the pinned Nix flake and expose `bin/sctui`. Deterministic regression tests run through `./test`; `./build` produces the release executable. Peter verifies interactive feel in his terminals. Live-service checks are opt-in and must not expose credentials.

These outcomes come from Peter's requests in this project's development conversation. Current tasks and completion evidence live in [PLAN.md](PLAN.md).
