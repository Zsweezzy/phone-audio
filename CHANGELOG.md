# Changelog

All notable changes to this project are documented in this file. Format
follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this
project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.4] - 2026-10-10

### Removed

- `phone-audio-core`: dead crate-root re-exports of the internal `cmd` runner
  and `pw` parser helpers (`CmdOut`, `CmdRunner`, `RealRunner`,
  `parse_profiles`, `parse_volume`, `pick_receive_profile`) — none of them had
  consumers outside the crate; callers import them via `crate::` paths.
- `phone-audio-core`: redundant empty-argv branch in `RealRunner::run` —
  `Command::new(prog).args([])` handles the empty case natively.

### Changed

- Repository hygiene: `Cargo.lock` is synced with the workspace version again
  (it had drifted to 0.1.2 after the 0.1.3 release because the lockfile was not
  committed with the version bump).

## [0.1.3] - 2026-10-10

### Fixed

- `phone-audio-core`: a stale `pw-loopback` (whose captured bluez input node
  name no longer exists — the node index increments on every stream restart)
  previously made `on`/`toggle` a no-op: "pid alive" was treated as "routing
  healthy". State now stores the loopback's node name alongside its pid;
  `status` reports `on` only when the stored node matches the live source
  node, and `turn_on` re-arms by killing the orphaned loopback and spawning a
  fresh one on the current node. One re-toggle heals legacy states.
- `phone-audio-gui`: the toggle now flips instantly (optimistic) with a small
  spinning indicator while the operation runs, instead of waiting for the
  reconnect/stream wait to finish.

### Added

- `phone-audio-gui`: visual redesign — status dot + state pill header, phone
  card with profile chip, volume row only while routing, color-coded status
  line, brighter muted text for contrast.
- `phone-audio-gui`: the phone picker auto-selects the first available device.
- `quickshell`: `PhoneAudio.setVolume` shim and a volume slider in the live
  flyout's PHONE AUDIO section (wired to `phone-audio volume`).

## [0.1.2] - 2026-10-10

### Fixed

- `phone-audio-core`: `phone-audio off` never aborts on state-write errors — a
  read-only state dir now warns and still kills the loopback, drops the profile
  and disconnects.
- `phone-audio-core`: status no longer rewrites `state.json` when the remembered
  phone is unchanged (it used to write every 1 s poll, racing the toggle
  worker's pid write on the same tmp file); state tmp writes now use a unique
  per-writer filename, so two writers can never rename over each other's
  in-flight bytes and lose the loopback pid.
- `phone-audio-cli`: new `phone-audio forget` command drops a stale remembered
  phone (`last_seen`) from `state.json` — after unpairing, status reports no
  phone again; `phone-audio debug` now shows `last_seen` and points at it.
- `phone-audio-core`: `phone-audio on` now reconnects a configured phone with
  `bluetoothctl connect` when it is absent (waiting up to ~15 s for its card),
  waits up to ~30 s for the phone to actually stream (polling every 500 ms,
  with a one-line "waiting for the phone to play" notice), and errors with an
  actionable message instead of failing when no phone is configured at all.
- `phone-audio-core`: `phone-audio off` now runs `bluetoothctl disconnect`
  after killing the loopback and dropping the profile, so the phone plays on
  its own speaker while staying paired; the profile drop and disconnect are
  best-effort and tolerate the card vanishing mid-turn-off.
- `phone-audio-core`: status reports "phone not connected — run 'phone-audio
  on' to reconnect" when the configured phone is absent (including after
  `off`), pointing at the toggle flow instead of re-selecting the phone.
- `phone-audio-gui`: run the on/off toggle on a background thread via
  `Task::perform` + `spawn_blocking` so the ~30 s wait no longer freezes the
  window; the status poll keeps ticking and repeat toggles are ignored while
  one is pending.
- `phone-audio-core`: the last seen phone (`mac`, `name`, `device_name`) is
  persisted in `state.json`, so after `phone-audio off` the phone stays
  `available` in `status` ("phone not connected — run 'phone-audio on' to
  reconnect") and `phone-audio on` reconnects the remembered phone even
  without a configured MAC.
- `phone-audio-core`: `phone-audio off` tolerates a vanished loopback pid
  (skips the kill, no crash — the old /proc check raced the kill) and a
  failing `kill` (warns, profile off + disconnect still complete).
- `phone-audio-gui`: a failed toggle no longer panics the window, and the
  status line shows "working…" while a toggle runs.

### Docs

- `README.md`: document the new on/off flow — `on` reconnects the phone if
  needed and waits up to ~30 s for streaming before routing, while `off`
  disconnects the phone so it plays on its own speaker (verified live on an
  iPhone) with the pairing kept; the Control-Center workaround is now just a
  footnote for phones that misbehave after `off`.
- `README.md`: document that after `off` the phone remains `available` in
  `status` (remembered id), so the GUI/quickshell toggle stays enabled and
  reconnects it.

## [0.1.1] - 2026-10-09

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