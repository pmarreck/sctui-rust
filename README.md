# sctui

[![Mechatron Prime CI](https://img.shields.io/endpoint?url=https%3A%2F%2Fthelio-nixos.tail66c90.ts.net%2Fbadges%2Fsctui-rust.json&style=for-the-badge)](https://thelio-nixos.tail66c90.ts.net/mechatron-prime/)

a soundcloud client for the terminal

![demo](./media/playing_demo.png)

## Authentication

On startup, `sctui` uses the first available credential source in this order:

1. An unexpired `token.json` in the current directory.
2. `SOUNDCLOUD_CLIENT_ID` and `SOUNDCLOUD_CLIENT_SECRET`, including values loaded from `.env`.
3. The `oauth_token` cookie from a signed-in SoundCloud session in a local Firefox profile.

Firefox fallback supports standard Firefox, Snap Firefox, and Flatpak Firefox profiles on Linux, plus the standard profile locations on macOS and Windows. It reads a private snapshot of `cookies.sqlite` and its WAL sidecars. Browser credentials are used only for the current process and are never written to `token.json`.

SoundCloud rejects browser-session tokens on several legacy `api.soundcloud.com` routes. For Firefox sessions, `sctui` resolves the signed-in user through api-v2 and uses its user-scoped likes, library, following, playlist, album, and followed-person track collections.

Search also uses api-v2 resources for Tracks, Albums, Playlists, and People. Album results recognize both legacy and api-v2 album discriminators.

Playback likewise uses the API-v2 transcoding metadata returned with each track. Playback failures are shown in red in the status line for at least six seconds while `sctui` advances through the manual queue and current collection until a track starts or no candidates remain. When SoundCloud marks a track as Go+ high-tier content, `sctui` reports that its encrypted playback is unsupported instead of displaying the 404 from SoundCloud's obsolete fallback resolver.

## Controls

In embedded terminals, unavailable graphics/font-size queries fall back to text halfblock artwork. The status line explains this capability gap once per session for six seconds, without restarting the warning on track changes or resizes. Cover-art download/encoding errors remain visible; these optional failures do not stop playback. Failed artwork URLs are attempted once per URL change, with a five-second request timeout. Mouse, colors, and title updates still depend on support from the host terminal.

The italic status line between the active list and player shows every keyboard shortcut in a scrolling marquee. Press `Shift+H` for the full help popup.

Press `Ctrl-Q` to exit immediately from any view or input field. `Esc` opens the quit confirmation.

Press `Shift+S` in Likes to toggle shuffle (the player shows `shf: ✔︎` when enabled). It shuffles the remaining playable queue without interrupting the current track or rearranging the list. Press it again to restore sequential playback. This also works for other library collections; on Search, uppercase `S` remains search text.

Encrypted-only metadata takes precedence over obsolete stream URLs. If a stream resolver still returns HTTP 404 without a known restriction, the error explains that DRM/Go+, regional restrictions, removed media, or an expired stream URL may be responsible; the response alone cannot establish which.

While audio is actively playing, the terminal tab title begins with `🔊`. Pausing, stopping, or exiting removes the speaker marker through the standard OSC 2 title sequence.

Mouse controls are also available:

- Click a main tab, library section, or search filter to select it.
- Click a playlist, album, or followed person to load its tracks.
- Click a track once to select it and twice within 400 ms to play it.
- Scroll the mouse wheel over the content to move track selection by one; hold Shift to move by five.
- Click the playback progress bar to restart the current track at that position.

Track clicks, wheel selection, and Enter playback use the same active track-pane target across Likes, Playlists, Albums, Following, and Search.

`Page Up` and `Page Down` move track selection by one visible page. On the Search tab, typing focuses the visible input cursor, Enter submits the query, and navigation moves focus to the results so Enter can play the selected track.

## Build and test

Run `./build` for the pinned, sandboxed release build. The executable is installed at `result/bin/sctui`; this checkout's `bin` symlink exposes it as `bin/sctui`.

Run the same build and test derivation used by Mechatron Prime with `./test`. The upstream project ran `cargo test` but contained no project tests at this fork's base commit; this fork's Rust tests cover authentication, API routes and parsing, playback, input, and deterministic TUI rendering. Its underlying command is:

```sh
nix build .#checks.x86_64-linux.default
```

The development shell includes Rustfmt; run `nix develop -c cargo fmt` to format Rust sources.

## Features

### 🎧 High Quality Ad-Free playback

<p align="center">Stream tracks directly from SoundCloud without the interruptions you would usually experience with a free account on the website</p>

### ✅ Fully Featured

<p align="center">Browse your own Likes, Playlists and Saved Albums as well as the Tracks and Likes of the People you follow</p>

<p align="center"><img src="./media/browse.png" alt="Browse" width="480" /></p>

<p align="center">Search for Tracks, Albums and Playlists to add to your library, as well as new People to follow</p>

<p align="center"><img src="./media/search_feature.png" alt="Search" width="480" /></p>

<p align="center">View the activity of everyone you follow to stay up to date with their latest releases or reposts</p>

<p align="center">🚧 Feature Coming Soon 🚧</p>

### 🔊 Gapless Playback

<p align="center">Enjoy seamless transitions in your favourite albums without the buffering present on SoundCloud Web</p>

### 👁️ Audio Visualiser

<p align="center">View the waveforms of your favourite music in oscilloscope or audio spectrum visualisation modes</p>

<p align="center"><img src="./media/visualiser.gif" alt="Visualiser" width="480" /></p>

<p align="center"><img src="./media/spectrum.gif" alt="Visualiser 2" width="480" /></p>

<p align="center">(maybe more modes coming soon..)</p>

## Limitations

### ❌ Playback of Go+ Tracks

- Due to SoundCloud API limitations, Go+ tracks are not playable from the application

### ❌ Downloads

- Due to the SoundCloud API Terms of Use, the download and offline playback of tracks is not supported

## Dev Diary

find the dev diary to follow along the development ~~struggle~~ process [here](./DEV_DIARY.md)

## License

Copyright (c) Will Murphy <contact@w-murphy.com>

This project is licensed under the MIT license ([LICENSE] or <http://opensource.org/licenses/MIT>)

[LICENSE]: ./LICENSE
