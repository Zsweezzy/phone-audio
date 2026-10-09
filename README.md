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

## How it works

1. `pw-dump` finds the connected `bluez5` card and its active profile.
2. `pw-cli enum-params <id> EnumProfile` lists profiles; the first available
   one that *captures* phone audio wins: `a2dp-source` → `a2dp-duplex` → any
   available profile exposing an `Audio/Source`.
3. `pactl set-card-profile` switches to it (skipped if already active).
4. It waits (up to ~2.5 s) for the `bluez_input.<mac>.1` source node, then
   starts `pw-loopback` from it to the default sink in the background and
   records the PID.
5. `turn_off` kills that PID and sets the profile back to `off`.

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
- If no `a2dp-source` profile is listed, inspect
  `pw-cli enum-params <device-id> EnumProfile` and make sure the WirePlumber
  bluetooth role config includes `a2dp_source` in `bluez5.roles` (the
  `api.bluez5.profile` of the card must be switchable to it).
- After `phone-audio on`, the `bluez_input.<mac>.*` source node only appears
  once the phone actually streams — if nothing happens, start playback on the
  phone (the status line repeats this as "not streaming — start playback on
  the phone").
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