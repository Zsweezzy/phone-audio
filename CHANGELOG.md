# Changelog

All notable changes to this project are documented in this file. Format
follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this
project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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