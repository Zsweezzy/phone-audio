# Changelog

All notable changes to this project are documented in this file. Format
follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this
project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- `phone-audio-core`: read the active bluez profile from `bluez5.profile`
  (the key pw-dump actually emits), falling back to `api.bluez5.profile`.
- `phone-audio-core`: pick a receive profile on real bluez cards, whose
  `EnumProfile` blocks carry no `classes` struct — profiles are now also
  recognized by name (`audio-gateway`, `handsfree`, ...); `phone-audio on`
  works on a card that only offers `off` + `audio-gateway`.
- `phone-audio.desktop`: launch the GUI with `env -u MANGOHUD` so the overlay
  doesn't cover the window when `MANGOHUD=1` is set session-wide.
- `phone-audio-core`: find the bluez input node by its `bluez_input.<mac>.*`
  name instead of `media.class = "Audio/Source"` — PipeWire 1.6.x exposes the
  streaming capture node as `Stream/Output/Audio`, so `on`/`volume` now work
  against a real phone.
- `phone-audio-core`: report `on` from the live loopback plus the present
  source node rather than the card's `bluez5.profile`, which stays `off` while
  streaming; status reasons now distinguish "streaming — run 'phone-audio on'",
  "profile off — run 'phone-audio on'" and "not streaming — start playback on
  the phone".

## [0.1.0] - 2026-10-09

Initial release.

### Added

- `phone-audio-core`: discovery via `pw-dump`, receive-profile picking from
  `pw-cli enum-params`, profile switch via `pactl`, background `pw-loopback`
  with persisted PID, status (phone / profile / on / volume), volume control
  via `wpctl`, phone selection by name or MAC, `debug` dump, and a single
  `CmdRunner` abstraction that makes everything testable without a phone.
- `phone-audio-cli`: `status [--json]`, `list [--json]`, `set-phone`, `on`,
  `off`, `toggle`, `volume`, `debug`.
- `phone-audio-gui`: iced window with phone picker, on/off toggle, volume
  slider, and a status line; polls every second.
- `quickshell/PhoneAudio.qml`: optional quickshell shim (singleton with a 1 s
  timer, talks to the CLI).
- `install.sh`, `phone-audio.desktop`, `assets/phone-audio.svg`.
- GitHub Actions release workflow (`v*` tags): release builds + tarball.