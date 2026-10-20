# Comments

All comments from the non-test Rust sources live here so the code files stay
comment-free. Every block is anchored to the code line it documented — search
for the anchor to locate it. Test narration stays next to its code:
`crates/phone-audio-core/src/app/tests.rs` and the inline `mod tests` in
`pw.rs`.


## crates/phone-audio-core/src/app.rs


**file header**

//! The application: discovery via `pw-dump`, profile switching, loopback, config/state.

`pub struct Phone {`
/// A discovered bluetooth phone (bluez5 card).

`pub struct Status {`
/// Current state, serializable for `phone-audio status --json`.

`pub volume: Option<f64>,`
    /// Volume 0..=100 as a percentage, None when no bluez source node exists yet.

`pub phones: Vec<Phone>,`
    /// All connected phones, from the same scan as the rest of the status.

`const RECONNECT_REASON: &str = "phone not connected — run 'phone-audio on' to reconnect";`
/// Shown when a configured phone is absent from the scan (also post-`off`).

`const SOURCE_POLL_INTERVAL: Duration = Duration::from_millis(500);`
/// Poll interval while waiting for a phone card / streaming source node.

`const WAIT_FOR_SOURCE_SECS: u64 = 30;`
/// `turn_on` waits up to this long for the phone to actually stream.

`const WAIT_FOR_CARD_SECS: u64 = 15;`
/// `turn_on` waits up to this long for a reconnected phone's card to appear.

`fn poll_count(total_secs: u64) -> u64 {`
/// Polls at [`SOURCE_POLL_INTERVAL`] that fit into `total_secs`.

`struct BlueDevice {`
/// A bluez card with its active profile.

`struct BlueNode {`
/// A bluez Audio/Source or Audio/Sink node.

`struct Scan {`
/// One `pw-dump` parse: cards, source nodes and sink nodes.

`struct LastSeen {`
/// A remembered phone, persisted in state.json so a disconnected phone stays
/// known: status keeps reporting it as available and `turn_on` can reconnect it.

`fn into_phone(self) -> Phone {`
    /// A displayable phone; the card id is unknown outside a scan, so 0.

`loopback_node: Option<String>,`
    /// The bluez_input source node the recorded loopback is capturing. The name
    /// increments when the phone's stream restarts, so a stored node different
    /// from the current one means the loopback is stale and must be re-armed.
    /// Absent in state files written before this field existed (→ None).

`pub struct App {`
/// The app. Holds its configuration/state paths and the command runner.

`pub fn new() -> Result<Self> {`
    /// Real runner, `$XDG_CONFIG_HOME/phone-audio` (fallback `~/.config/phone-audio`).

`pub(crate) fn with_runner(runner: Box<dyn CmdRunner>, phone_mac: Option<String>) -> Self {`
    /// For tests: caller-provided runner and no persisted phone selection.

`pub(crate) fn with_paths(`
    /// For tests: full control over config/state paths.

`pub fn list_phones(&mut self) -> Result<Vec<Phone>> {`
    /// All connected bluez phones, deduped by MAC, sorted by name.

`pub fn status(&mut self) -> Result<Status> {`
    /// Full status: phone, active profile, whether our loopback is running, volume.

`if let Some(known) = self.known_phone() {`
            // A known-but-disconnected phone (configured MAC, or remembered in
            // state after `off`) is still available: the reason points at the
            // reconnect flow and consumers keep the toggle enabled. Only when
            // nothing is known is the phone truly gone.

`if let Some(p) = self`
        // Remember the phone we'd pick (or the first one for an unconfigured
        // multi-phone scan) so `off` and a later reconnect still know it once
        // the card is gone. Best-effort: state is disposable and a read-only
        // config dir must not break status. Only rewritten when the remembered
        // phone actually changed — the GUI polls status every second and a
        // no-op rewrite would race the toggle worker's pid write for nothing.

`let node_matches = state`
        // on = our loopback is live AND the phone's source node exists AND the
        // loopback is still pointed at that node. The bluez5.profile stays "off"
        // even while streaming (it reflects the idle BlueZ connection state), so
        // it can't drive the on/off decision. When the phone's stream restarts
        // the bluez_input node name increments, leaving the old loopback on a
        // dead name: that must read off, not a stale "on". A legacy state with
        // no stored node reads off once; the next toggle re-arms it with the
        // current node, permanently self-healing.

`pub fn turn_on(&mut self) -> Result<()> {`
    /// Switch to a receive profile, wait for the bluez input node, run the loopback.

`let target = self`
            // Reconnect target: the configured MAC, else the last remembered
            // phone (so the unconfigured GUI/bar toggle works after `off`).

`if let Err(e) = self.run(&["bluetoothctl", "connect", &mac]) {`
                    // Best-effort connect: the phone may be genuinely absent,
                    // in which case wait_for_card surfaces the real outcome.

`let msg = if scan.devices.is_empty() {`
                    // Nothing to reconnect to: stop here with an actionable
                    // error instead of guessing a MAC.

`let state = load_state(&self.state_path);`
        // "Already on" means the recorded loopback is live AND still pointed at
        // the phone's current source node. When the stream restarts, the
        // bluez_input node name increments (bluez_input.<mac>.1 -> .2 -> ...)
        // and the old loopback keeps capturing a dead name while state still
        // records it: that combination must re-arm, not no-op. A recorded but
        // dead/zombie pid must also re-arm. Otherwise kill the stale live
        // loopback best-effort (exactly like turn_off: /proc is the final word
        // and a failure must never abort the flow), then spawn a fresh loopback
        // against the current node — repeated `on` while streaming re-arms
        // correctly and never double-runs.

`pub fn turn_off(&mut self) -> Result<()> {`
    /// Kill our loopback, drop the profile, and disconnect the phone so it
    /// plays on its own speaker (pairing is kept).

`if Path::new(&format!("/proc/{pid}")).exists() {`
                // The loopback can exit right between the alive-check and the
                // kill (it dies when the phone's stream drops, which off is
                // about to cause): /proc/<pid> is the final word, and a gone
                // pid must not abort the off-flow. A kill that fails for any
                // other reason is equally non-fatal — the loopback is already
                // dead or dying, so warn and carry on with the disconnect.

`eprintln!("phone-audio: warning: could not clear state: {e}");`
                // W2: state is disposable and must never stop `off` from
                // actually disconnecting (a read-only state dir must not keep
                // the phone streaming). Warn and carry on.

`let scanned_mac = self.scan().ok().and_then(|scan| {`
        // Best-effort profile drop: the card can vanish mid-turn-off (the
        // disconnect below races it away), so a missing device is not an error.

`if let Err(e) = self.run(&["pactl", "set-card-profile", &dev.phone.device_name, "off"])`
            // Non-fatal: switching to "off" when it already is off can be non-zero,
            // but the user must know if the drop actually failed.

`let mac = self.phone_mac.clone().or(scanned_mac);`
        // Disconnect so the phone's media falls back to its own speaker. The
        // MAC comes from config or the (now possibly gone) scan; tolerating a
        // failed disconnect keeps this best-effort when the device vanished.

`pub fn toggle(&mut self) -> Result<bool> {`
    /// status -> on ? turn_off : turn_on; returns the new `on` state.

`pub fn forget(&mut self) -> Result<()> {`
    /// Drop the remembered phone (`last_seen`) from state, keeping the loopback
    /// pid untouched: after a phone is unpaired, a stale last_seen would keep
    /// reporting it as available forever. While off (no live loopback), status
    /// then reports no phone again until the next time it is seen.

`pub fn set_volume(&mut self, pct: f64) -> Result<()> {`
    /// Set volume 0..=100 on the phone's bluez Audio/Source node.

`pub fn set_phone(&mut self, name_or_mac: &str) -> Result<Phone> {`
    /// Persist a phone selection matched by MAC or name (both case-insensitive).

`pub fn debug(&mut self) -> Result<String> {`
    /// Multi-line troubleshooting dump.

`fn run(&mut self, args: &[&str]) -> Result<CmdOut> {`
    // ---- internals ---------------------------------------------------------
    /// Blocking run with a non-zero-exit check.

`fn scan(&mut self) -> Result<Scan> {`
    /// One `pw-dump` parse covering cards, source nodes and sink nodes.

`let Some(mac) = p.get("api.bluez5.address").and_then(|x| x.as_str()) else {`
                    // Guard: a card without an address is not a usable remote phone.

`if node.name.starts_with("bluez_input.") || media == Some("Audio/Source") {`
                    // PipeWire 1.6.x emits the bluez input node as
                    // Stream/Output/Audio (not Audio/Source), so the name prefix
                    // is the reliable signal; the class-based push stays for
                    // other configurations.

`fn known_phone(&self) -> Option<Phone> {`
    /// The phone we know even when it is absent from the scan: the configured
    /// MAC (preferring a matching last_seen record for its display info), else
    /// the remembered last_seen phone. None when nothing is known at all.

`fn wait_for_card(&mut self, mac: &str) -> Result<()> {`
    /// Poll `pw-dump` up to [`WAIT_FOR_CARD_SECS`] for the configured phone's card.

`fn wait_for_source(&mut self, mac: &str) -> Result<Option<String>> {`
    /// Poll `pw-dump` up to [`WAIT_FOR_SOURCE_SECS`] for the
    /// `bluez_input.<mac>.*` source node, announcing the wait once when it
    /// actually has to block on the phone starting to stream.

`fn default_sink(&mut self) -> Result<String> {`
    /// `pactl get-default-sink`, falling back to parsing `wpctl status` for the `*` row.

`fn save_last_seen_if_changed(&self, phone: &Phone) -> Result<()> {`
    /// Like [`Self::save_last_seen`], but skips the write when the stored
    /// record already matches: `status()` is polled once a second and must not
    /// rewrite state.json on every poll (a no-op rewrite would race the toggle
    /// worker's own rename on the same tmp file for zero benefit).

`fn save_state(&self, update: impl FnOnce(&mut StateFile)) -> Result<()> {`
    /// Load-modify-save on state.json, preserving fields other than the one
    /// being updated (a pid write must not drop the remembered phone and vice
    /// versa).

`fn clear_state(&self) -> Result<()> {`
    /// Drop the loopback pid but keep `last_seen`: a disconnected-but-known
    /// phone must stay available so the toggle can reconnect it after `off`.
    /// Without a last_seen the file is simply removed (nothing left to keep).

`fn tmp_path(path: &Path) -> PathBuf {`
/// A tmp path unique per writer per call: the GUI's 1/sec status poll and a
/// toggle worker both write state.json from the same process, so a shared
/// tmp name would let one writer's rename clobber the other's in-flight bytes
/// (lost update -> dropped loopback pid -> `on` later spawns a duplicate
/// pw-loopback). The rename-to-final stays atomic; each writer's bytes are
/// complete before its own rename.

`fn load_config(path: &Path) -> Result<Option<String>> {`
/// Missing or malformed file -> None (no selection; next set-phone rewrites it).

`fn load_state(path: &Path) -> StateFile {`
/// State is disposable: any read problem just means "no state".

`fn load_pid(path: &Path) -> Option<u32> {`
/// Missing or malformed state -> no loopback pid.

`fn load_last_seen(path: &Path) -> Option<LastSeen> {`
/// Missing or malformed state -> no remembered phone.

`fn pid_alive(pid: u32) -> bool {`
/// Process-alive check; Linux-only per spec (`/proc/<pid>`).

`let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).unwrap_or_default();`
    // Identity: must actually be our loopback.

`std::fs::read_to_string(format!("/proc/{pid}/stat"))`
    // /proc/<pid>/stat field 3 = process state char; 'Z' = zombie (exited, unreaped).

`scan.sources`
    // Prefer the real bluez input node over its `.monitor` sibling.

`fn is_on(pid_alive: bool, source: bool) -> bool {`
/// The on/off decision: our loopback pid is alive `&&` the phone's bluez input
/// source node exists (profile is idle-state info and can't carry this).

`mod tests;`
// tests live in tests.rs to keep the fixture JSON out of this file's noise


## crates/phone-audio-core/src/cmd.rs


`pub struct CmdOut {`
/// Captured output of a blocking command run.

`pub trait CmdRunner: Send {`
/// The only abstraction this crate allows: every external command goes through it.

`fn run(&mut self, args: &[&str]) -> Result<CmdOut>;`
    /// Blocking run; argv[0] is the executable, captures stdout/stderr.

`fn run_detached(&mut self, args: &[&str]) -> Result<u32>;`
    /// Spawn a background process (stdio to /dev/null); returns its PID.

`fn box_clone(&self) -> Box<dyn CmdRunner>;`
    /// A fresh boxed clone. Lets the GUI run a toggle on a detached copy of the
    /// app (its runner is stateless; state that matters lives in files).

`pub struct RealRunner;`
/// Runs real processes via std.


## crates/phone-audio-core/src/lib.rs


**file header**

//! phone-audio-core — routes a Bluetooth phone's audio to this PC via PipeWire.
//!
//! Everything shells out through `CmdRunner`; nothing talks to PipeWire directly.

`pub enum AudioError {`
/// Errors produced by the app.


## crates/phone-audio-core/src/pw.rs


**file header**

//! Parsing helpers for the output of `pw-cli enum-params`, `pw-dump`, `wpctl`.

`pub struct Profile {`
/// One entry of a `pw-cli enum-params <id> EnumProfile` block.

`pub available: bool,`
    /// `available: Id 2` means the profile is available (SPA_PARAM_AVAILABILITY_yes).

`pub has_source: bool,`
    /// The block's `classes` struct contains `String "Audio/Source"`.

`pub const RECEIVE_PROFILES: [&str; 6] = [`
/// Profile names that receive phone (remote) audio. Bluez cards emit
/// class-less `EnumProfile` blocks (PipeWire 1.6.x), so a profile is also
/// receive-capable when its name is in this list — not just when it exposes an
/// `Audio/Source` class. Single source of truth for profile-name matching.

`pub fn parse_profiles(text: &str) -> Vec<Profile> {`
/// Parse `pw-cli enum-params ... EnumProfile` text (the pod-dump format) into
/// profile blocks. Tolerant: only looks for the interesting lines.
///
/// A new profile starts at a line containing both `Object:` and `Param:Profile`;
/// any other `Object:` line (a foreign block) closes the current profile so its
/// props cannot leak into a neighboring one. Within a profile, `Prop: key
/// ...:index/name/available` lines are followed by `Int N` / `String "..."` /
/// `Id N` value lines. `Audio/Source` appearing in the block marks the profile
/// as source-capable.

`key = rest`
            // `Spa:Pod:Object:Param:Profile:index (1), flags ...` -> `index`

`let mut it = t.split_whitespace();`
                // `Id 2 (Spa:Enum:ParamAvailability:yes)` -> available yes

`pub fn pick_receive_profile(profiles: &[Profile]) -> Option<&Profile> {`
/// Pick the profile that receives phone (remote) audio:
/// available `a2dp-source`, else `a2dp-duplex`, else `audio-gateway`, else any
/// available source-capable profile (Audio/Source class **or** name in
/// [`RECEIVE_PROFILES`]), preferring `a2dp`-named ones over the first capable.

`pub fn parse_volume(text: &str) -> Option<f64> {`
/// Extract the volume float from `wpctl get-volume` output (e.g. `Volume: 0.65`).

`pub fn props(v: &Value) -> Option<&Map<String, Value>> {`
/// `info.props` of a pw-dump object.

`pub fn obj_type(v: &Value) -> Option<&str> {`
/// The pipewire object type string ("PipeWire:Interface:Device", ...).

`pub fn obj_id(v: &Value) -> u32 {`
/// The pipewire object id.

`pub fn mac_underscored(mac: &str) -> String {`
/// `28:8F:F6:71:6F:6E` -> `28_8F_F6_71_6F_6E` (the form used in node/device names).


## crates/phone-audio-gui/src/main.rs


**file header**

//! phone-audio GUI — a small always-on-top-style window to route a Bluetooth
//! phone's audio to this PC. Polls status every second via a subscription.

`let status = iced::time::every(Duration::from_secs(1)).map(|_| Msg::Tick);`
            // 1/s status poll always; while a toggle is in flight, also spin the
            // loading arc fast enough to look like motion.

`busy: bool,`
    /// A toggle is running off the UI thread; ignore further toggles until it lands.

`pending: Option<bool>,`
    /// Optimistic toggle target while the off-thread toggle runs: the toggler
    /// shows this instead of the (still stale) status so the flip is instant.
    /// Only `Msg::ToggleDone` clears it — the 1/s status refresh must not.

`spinner_phase: usize,`
    /// Index into the loading-arc glyphs; advanced by `Msg::SpinnerTick`.

`const SPINNER_GLYPHS: [char; 4] = ['◜', '◝', '◞', '◟'];`
/// Rotating arc glyphs, one per 100 ms tick: reads as a spinning arc, renders
/// with the normal font (no drawing widget / no feature needed).

`if self.busy {`
                // One toggle at a time; the wait can block ~30 s, so it runs
                // off the UI thread on a clone of the app (state that matters
                // lives in files, so the polling copy stays consistent). The
                // toggler stays visually enabled while busy, so ignore the
                // click here — the flipped state is already shown.

`self.pending = Some(!self.pending.unwrap_or(self.status.on));`
                // Flip instantly: show the target the moment the click lands.

`.map_err(|e| format!("toggle worker failed: {e}"))?`
                        // A panicked worker must not wedge the toggle: surface
                        // it as an error instead; busy clears in ToggleDone.

`self.phones = s.phones.clone();`
// same scan status() already performed

`if !options.is_empty()`
        // Auto-select the first phone when nothing is selected yet or the
        // selection vanished from the scan. Display-only: never calls
        // set_phone/App on auto-select.

`if self.combo.options() != options.as_slice() {`
        // Rebuild only on an actual change so an open dropdown survives ticks.

`if self.status.available {`
        // Visually enabled even while busy: the defensive busy guard in
        // update() ignores the click, and the flipped value is already shown.

`"working…".to_string()`
            // The toggle can wait up to ~30 s (reconnect + streaming): say so
            // instead of showing the stale reason.

`let has_error = self.error.is_some() || self.startup_error.is_some();`
        // --- Tailors grouped by current state --------------------------------

`("working…", CYAN, CYAN_TINT)`
            // An in-flight turn-off outlives status.on by up to ~30 s; busy wins.

`let header = row![`
        // --- Header: dot, title, spacer, state pill --------------------------

`let profile = self.status.profile.as_deref().unwrap_or("-");`
        // --- Phone card: picker + profile chip -------------------------------

`toggle_row = toggle_row.push(`
            // Small (~14px) rotating arc next to the toggler while the
            // off-thread toggle runs; nothing rendered when not busy.

`if self.status.on {`
        // Volume is only meaningful while audio is actually routed.

`const FG: Color = Color::from_rgba8(0xc0, 0xca, 0xf5, 1.0);`
// --- Tokyonight-night palette -------------------------------------------------

`fn card() -> ContainerStyle {`
/// Full-width rounded card with a subtle border.

`fn pill(bg: Color) -> ContainerStyle {`
/// Fully-rounded pill (chip / state badge) with no visible border.


## crates/phone-audio-cli/src/main.rs

