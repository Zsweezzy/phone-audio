# Phone Audio

Route a Bluetooth phone's audio to this PC via PipeWire. Say "on" and your
phone's music plays through the PC speakers; say "off" and it plays on the
phone again.

```
┌── phone ──A2DP──┐   pw-loopback    ┌── PC ──┐
│ (source profile)│ ───────────────► │ speakers│
└─────────────────┘                   └────────┘
```

Under the hood: `pactl set-card-profile ... a2dp-source`, then a detached
`pw-loopback -C bluez_input.<mac>.* -P <default-sink>`. The loopback PID is
remembered in `~/.config/phone-audio/state.json` so `off` can kill exactly it.

## Install

From a release tarball:

```sh
./install.sh          # installs to ~/.local/bin + ~/.local/share
```

From the repo (it also builds the release binaries):

```sh
./install.sh
```

Requires `pipewire`, `wireplumber`, `pulseaudio-utils` (for `pactl`), and a
PipeWire config where bluez `a2dp_source` is enabled (the default with
WirePlumber). The GUI additionally needs a Wayland/X11 session.

## Usage

```
phone-audio on          # switch phone to a2dp-source and start the loopback
phone-audio off         # stop the loopback and drop the profile (phone plays again)
phone-audio status      # phone, profile, on/off, volume
phone-audio status --json
phone-audio list        # connected bluetooth phones
phone-audio set-phone NAME_OR_MAC   # remember which phone (name or MAC)
phone-audio volume 80   # set phone volume 0-100
phone-audio toggle
phone-audio debug       # troubleshooting dump
phone-audio-gui         # small window with phone picker, toggle and volume
```

State and selection live in `~/.config/phone-audio/{config,state}.json`.

> **GUI + MangoHUD**: if `MANGOHUD=1` is set session-wide, launch the GUI with
> `env -u MANGOHUD phone-audio-gui` so the overlay doesn't cover the window
> (the desktop entry already does this).

## How it works

1. `pw-dump` finds the connected `bluez5` card and its active profile.
2. `pw-cli enum-params <id> EnumProfile` lists profiles; the first available
   one that *captures* phone audio wins: `a2dp-source` → `a2dp-duplex` →
   `audio-gateway` → any available profile exposing an `Audio/Source` class or
   a known receive profile name (bluez cards don't emit class structs).
3. `pactl set-card-profile` switches to it (skipped if already active).
4. It waits (up to ~2.5 s) for the `bluez_input.<mac>.*` source node, then
   starts `pw-loopback` from it to the default sink in the background and
   records the PID. This node only appears while the phone actually streams,
   so run `on` while/after playback starts — it waits ~2.5 s — or press play
   on the phone first, then run `on`.
5. `turn_off` kills that PID and sets the profile back to `off` — whether the
   phone then falls back to its own speaker depends on the phone's media
   stack (iOS may keep the session silent until you pick the phone as
   output, see Troubleshooting).

Only one abstraction is allowed out of the core: every external command goes
through `CmdRunner`. That is what makes the whole app testable without a
phone — the tests script a fake runner.

## Quickshell integration

```sh
cp quickshell/PhoneAudio.qml ~/.config/quickshell/<your-config>/
```

Quickshell can't import QML from outside its own folder, so the file must live
inside the config. Once copied, `PhoneAudio` resolves as a type anywhere in the
config (it's a `pragma Singleton`); the panel polls `phone-audio status --json`
itself every second.

```qml
Text { text: PhoneAudio.summary }
Button { onClicked: PhoneAudio.toggle() }
```

## Troubleshooting

- Run `phone-audio debug` first — it dumps phones, nodes, profiles and the
  loopback state in one place.
- If no receive profile is listed, inspect
  `pw-cli enum-params <device-id> EnumProfile` and make sure the WirePlumber
  bluetooth role config includes `a2dp_source` in `bluez5.roles` (the
  `bluez5.profile` of the card must be switchable to it).
- After `phone-audio on`, the `bluez_input.<mac>.*` source node only appears
  once the phone actually streams — start playback if `on` times out. The
  status line names the missing step: "streaming — run 'phone-audio on'"
  (node present, loopback not started), "profile off — run 'phone-audio on'",
  or "not streaming — start playback on the phone".
- While streaming, the card's `bluez5.profile` may still read `off` — that is
  normal idle BlueZ state. `status` reports `on: true` from the live loopback
  plus the present source node, not from the profile.
- After `phone-audio off` the phone may stay silent: iOS keeps its audio
  routed to the (now dropped) Bluetooth path and does not fall back to its
  own speaker until you pick the phone as output in the phone's audio picker
  (e.g. Control Center) or restart playback. This is the phone's media stack,
  not the PC — no A2DP transport exists on the PC side while off (check
  `pw-dump`). The device stays paired: run `phone-audio on` again with
  playback started or starting and routing is restored. Many Android phones
  resume on their own speaker immediately and need none of this.
- `phone-audio status --json` is the machine-readable surface for scripting
  and panels.

## Development

```sh
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check
```

Structure:

```
crates/phone-audio-core   logic + parsers + CmdRunner abstraction (tested, no phone needed)
crates/phone-audio-cli    clap frontend
crates/phone-audio-gui    iced frontend (poll every second)
quickshell/PhoneAudio.qml quickshell shim (optional watch widget)
```

## License

MIT unless otherwise noted — see [LICENSE](LICENSE).