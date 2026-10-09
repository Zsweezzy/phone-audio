//! The application: discovery via `pw-dump`, profile switching, loopback, config/state.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::cmd::{CmdOut, CmdRunner, RealRunner};
use crate::pw::{
    mac_underscored, obj_id, obj_type, parse_profiles, parse_volume, pick_receive_profile, props,
};
use crate::{AudioError, Result};

/// A discovered bluetooth phone (bluez5 card).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Phone {
    pub mac: String,
    pub name: String,
    pub device_name: String,
    pub device_id: u32,
}

impl std::fmt::Display for Phone {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.name, self.mac)
    }
}

/// Current state, serializable for `phone-audio status --json`.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Status {
    pub available: bool,
    pub phone: Option<Phone>,
    pub on: bool,
    pub profile: Option<String>,
    /// Volume 0..=100 as a percentage, None when no bluez source node exists yet.
    pub volume: Option<f64>,
    pub reason: String,
    /// All connected phones, from the same scan as the rest of the status.
    pub phones: Vec<Phone>,
}

const NO_PHONE_REASON: &str =
    "no bluetooth phone connected (pair first; bluetoothctl paired-devices)";
const MULTI_PHONE_REASON: &str = "multiple phones — pick one with 'phone-audio set-phone'";
/// Shown when a configured phone is absent from the scan (also post-`off`).
const RECONNECT_REASON: &str = "phone not connected — run 'phone-audio on' to reconnect";

/// Poll interval while waiting for a phone card / streaming source node.
const SOURCE_POLL_INTERVAL: Duration = Duration::from_millis(500);
/// `turn_on` waits up to this long for the phone to actually stream.
const WAIT_FOR_SOURCE_SECS: u64 = 30;
/// `turn_on` waits up to this long for a reconnected phone's card to appear.
const WAIT_FOR_CARD_SECS: u64 = 15;

/// Polls at [`SOURCE_POLL_INTERVAL`] that fit into `total_secs`.
fn poll_count(total_secs: u64) -> u64 {
    total_secs * 1000 / SOURCE_POLL_INTERVAL.as_millis() as u64
}

/// A bluez card with its active profile.
#[derive(Debug, Clone)]
struct BlueDevice {
    phone: Phone,
    profile: Option<String>,
}

/// A bluez Audio/Source or Audio/Sink node.
#[derive(Debug, Clone)]
struct BlueNode {
    id: u32,
    name: String,
}

/// One `pw-dump` parse: cards, source nodes and sink nodes.
#[derive(Debug, Default)]
struct Scan {
    devices: Vec<BlueDevice>,
    sources: Vec<BlueNode>,
    sinks: Vec<BlueNode>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ConfigFile {
    #[serde(default)]
    phone_mac: Option<String>,
}

/// A remembered phone, persisted in state.json so a disconnected phone stays
/// known: status keeps reporting it as available and `turn_on` can reconnect it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct LastSeen {
    mac: String,
    name: String,
    device_name: String,
}

impl LastSeen {
    /// A displayable phone; the card id is unknown outside a scan, so 0.
    fn into_phone(self) -> Phone {
        Phone {
            mac: self.mac,
            name: self.name,
            device_name: self.device_name,
            device_id: 0,
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct StateFile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    loopback_pid: Option<u32>,
    /// The bluez_input source node the recorded loopback is capturing. The name
    /// increments when the phone's stream restarts, so a stored node different
    /// from the current one means the loopback is stale and must be re-armed.
    /// Absent in state files written before this field existed (→ None).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    loopback_node: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_seen: Option<LastSeen>,
}

/// The app. Holds its configuration/state paths and the command runner.
pub struct App {
    runner: Box<dyn CmdRunner>,
    cfg_path: PathBuf,
    state_path: PathBuf,
    phone_mac: Option<String>,
}

impl Clone for App {
    fn clone(&self) -> Self {
        Self {
            runner: self.runner.box_clone(),
            cfg_path: self.cfg_path.clone(),
            state_path: self.state_path.clone(),
            phone_mac: self.phone_mac.clone(),
        }
    }
}

impl App {
    /// Real runner, `$XDG_CONFIG_HOME/phone-audio` (fallback `~/.config/phone-audio`).
    pub fn new() -> Result<Self> {
        let dir = config_dir();
        let phone_mac = load_config(&dir.join("config.json"))?;
        Ok(Self {
            runner: Box::new(RealRunner),
            cfg_path: dir.join("config.json"),
            state_path: dir.join("state.json"),
            phone_mac,
        })
    }

    /// For tests: caller-provided runner and no persisted phone selection.
    #[cfg(test)]
    pub(crate) fn with_runner(runner: Box<dyn CmdRunner>, phone_mac: Option<String>) -> Self {
        let dir = PathBuf::from(".");
        Self {
            runner,
            cfg_path: dir.join("config.json"),
            state_path: dir.join("state.json"),
            phone_mac,
        }
    }

    /// For tests: full control over config/state paths.
    #[cfg(test)]
    pub(crate) fn with_paths(
        runner: Box<dyn CmdRunner>,
        cfg_path: PathBuf,
        state_path: PathBuf,
        phone_mac: Option<String>,
    ) -> Self {
        Self {
            runner,
            cfg_path,
            state_path,
            phone_mac,
        }
    }

    /// All connected bluez phones, deduped by MAC, sorted by name.
    pub fn list_phones(&mut self) -> Result<Vec<Phone>> {
        Ok(self.scan()?.devices.into_iter().map(|d| d.phone).collect())
    }

    /// Full status: phone, active profile, whether our loopback is running, volume.
    pub fn status(&mut self) -> Result<Status> {
        let scan = self.scan()?;
        let phones: Vec<Phone> = scan.devices.iter().map(|d| d.phone.clone()).collect();
        if scan.devices.is_empty() {
            // A known-but-disconnected phone (configured MAC, or remembered in
            // state after `off`) is still available: the reason points at the
            // reconnect flow and consumers keep the toggle enabled. Only when
            // nothing is known is the phone truly gone.
            if let Some(known) = self.known_phone() {
                return Ok(Status {
                    available: true,
                    phone: Some(known.clone()),
                    phones: vec![known],
                    reason: RECONNECT_REASON.into(),
                    ..Default::default()
                });
            }
            return Ok(Status {
                available: false,
                reason: NO_PHONE_REASON.into(),
                phones,
                ..Default::default()
            });
        }
        // Remember the phone we'd pick (or the first one for an unconfigured
        // multi-phone scan) so `off` and a later reconnect still know it once
        // the card is gone. Best-effort: state is disposable and a read-only
        // config dir must not break status. Only rewritten when the remembered
        // phone actually changed — the GUI polls status every second and a
        // no-op rewrite would race the toggle worker's pid write for nothing.
        if let Some(p) = self
            .pick_phone(&scan)
            .or_else(|| scan.devices.first())
            .map(|d| &d.phone)
        {
            let _ = self.save_last_seen_if_changed(p);
        }
        let Some(dev) = self.pick_phone(&scan) else {
            let reason = if self.phone_mac.is_some() {
                RECONNECT_REASON
            } else {
                MULTI_PHONE_REASON
            };
            return Ok(Status {
                available: true,
                reason: reason.into(),
                phones,
                ..Default::default()
            });
        };
        let profile = dev.profile.clone();
        let state = load_state(&self.state_path);
        let pid = state.loopback_pid;
        let pid_alive = pid.is_some_and(pid_alive);
        let source = find_source(&scan, &dev.phone.mac);
        // on = our loopback is live AND the phone's source node exists AND the
        // loopback is still pointed at that node. The bluez5.profile stays "off"
        // even while streaming (it reflects the idle BlueZ connection state), so
        // it can't drive the on/off decision. When the phone's stream restarts
        // the bluez_input node name increments, leaving the old loopback on a
        // dead name: that must read off, not a stale "on". A legacy state with
        // no stored node reads off once; the next toggle re-arms it with the
        // current node, permanently self-healing.
        let node_matches = state
            .loopback_node
            .as_deref()
            .is_some_and(|stored| source.is_some_and(|n| n.name == stored));
        let on = is_on(pid_alive, source.is_some()) && node_matches;
        let volume = match source {
            Some(node) => self
                .run(&["wpctl", "get-volume", &node.id.to_string()])
                .ok()
                .and_then(|o| parse_volume(&o.stdout))
                .map(|v| (v * 100.0).clamp(0.0, 100.0)),
            None => None,
        };
        let reason = if on {
            String::new()
        } else if source.is_some() {
            "streaming — run 'phone-audio on'".into()
        } else if profile.as_deref() == Some("off") {
            "profile off — run 'phone-audio on'".into()
        } else {
            "not streaming — start playback on the phone".into()
        };
        Ok(Status {
            available: true,
            phone: Some(dev.phone.clone()),
            on,
            profile,
            volume,
            reason,
            phones,
        })
    }

    /// Switch to a receive profile, wait for the bluez input node, run the loopback.
    pub fn turn_on(&mut self) -> Result<()> {
        let mut scan = self.scan()?;
        if self.pick_phone(&scan).is_none() {
            // Reconnect target: the configured MAC, else the last remembered
            // phone (so the unconfigured GUI/bar toggle works after `off`).
            let target = self
                .phone_mac
                .clone()
                .or_else(|| load_last_seen(&self.state_path).map(|p| p.mac));
            match target {
                Some(mac) => {
                    // Best-effort connect: the phone may be genuinely absent,
                    // in which case wait_for_card surfaces the real outcome.
                    if let Err(e) = self.run(&["bluetoothctl", "connect", &mac]) {
                        eprintln!("phone-audio: warning: bluetoothctl connect {mac}: {e}");
                    }
                    self.wait_for_card(&mac)?;
                    scan = self.scan()?;
                }
                None => {
                    // Nothing to reconnect to: stop here with an actionable
                    // error instead of guessing a MAC.
                    let msg = if scan.devices.is_empty() {
                        "no bluetooth phone connected — pair it first, then run 'phone-audio on'"
                            .to_string()
                    } else {
                        MULTI_PHONE_REASON.to_string()
                    };
                    return Err(AudioError::NotFound(msg));
                }
            }
        }
        let Some(dev) = self.pick_phone(&scan) else {
            let msg = if scan.devices.is_empty() {
                "no bluetooth phone connected".to_string()
            } else {
                MULTI_PHONE_REASON.to_string()
            };
            return Err(AudioError::NotFound(msg));
        };
        let phone = &dev.phone;

        let out = self.run(&[
            "pw-cli",
            "enum-params",
            &phone.device_id.to_string(),
            "EnumProfile",
        ])?;
        let profiles = parse_profiles(&out.stdout);
        let chosen = pick_receive_profile(&profiles).ok_or_else(|| {
            AudioError::NotFound(
                "no receive-capable profile (need a2dp-source; check WirePlumber bluez5.roles includes a2dp_source)"
                    .into(),
            )
        })?;
        if dev.profile.as_deref() != Some(chosen.name.as_str()) {
            self.run(&[
                "pactl",
                "set-card-profile",
                &phone.device_name,
                &chosen.name,
            ])?;
        }

        let Some(node_name) = self.wait_for_source(&phone.mac)? else {
            return Err(AudioError::NotFound(format!(
                "no bluez_input source node for {} after switching profile — start playback on the phone",
                phone.name
            )));
        };

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
        let state = load_state(&self.state_path);
        let pid = state.loopback_pid;
        let already_on = pid.is_some_and(pid_alive)
            && state.loopback_node.as_deref() == Some(node_name.as_str());
        if !already_on {
            if let Some(pid) = pid {
                if pid_alive(pid) && Path::new(&format!("/proc/{pid}")).exists() {
                    if let Err(e) = self.run(&["kill", &pid.to_string()]) {
                        eprintln!("phone-audio: warning: could not kill stale loopback {pid}: {e}");
                    }
                }
            }
            let sink = self.default_sink()?;
            let pid = self.run_detached(&["pw-loopback", "-C", &node_name, "-P", &sink])?;
            self.save_state_pid(pid, &node_name)?;
        }
        Ok(())
    }

    /// Kill our loopback, drop the profile, and disconnect the phone so it
    /// plays on its own speaker (pairing is kept).
    pub fn turn_off(&mut self) -> Result<()> {
        if let Some(pid) = load_pid(&self.state_path) {
            if pid_alive(pid) {
                // The loopback can exit right between the alive-check and the
                // kill (it dies when the phone's stream drops, which off is
                // about to cause): /proc/<pid> is the final word, and a gone
                // pid must not abort the off-flow. A kill that fails for any
                // other reason is equally non-fatal — the loopback is already
                // dead or dying, so warn and carry on with the disconnect.
                if Path::new(&format!("/proc/{pid}")).exists() {
                    if let Err(e) = self.run(&["kill", &pid.to_string()]) {
                        eprintln!("phone-audio: warning: could not kill loopback {pid}: {e}");
                    }
                }
            }
            self.clear_state().unwrap_or_else(|e| {
                // W2: state is disposable and must never stop `off` from
                // actually disconnecting (a read-only state dir must not keep
                // the phone streaming). Warn and carry on.
                eprintln!("phone-audio: warning: could not clear state: {e}");
            });
        }
        // Best-effort profile drop: the card can vanish mid-turn-off (the
        // disconnect below races it away), so a missing device is not an error.
        let scanned_mac = self.scan().ok().and_then(|scan| {
            let dev = self.pick_phone(&scan)?;
            // Non-fatal: switching to "off" when it already is off can be non-zero,
            // but the user must know if the drop actually failed.
            if let Err(e) = self.run(&["pactl", "set-card-profile", &dev.phone.device_name, "off"])
            {
                eprintln!("phone-audio: warning: could not drop profile: {e}");
            }
            Some(dev.phone.mac.clone())
        });
        // Disconnect so the phone's media falls back to its own speaker. The
        // MAC comes from config or the (now possibly gone) scan; tolerating a
        // failed disconnect keeps this best-effort when the device vanished.
        let mac = self.phone_mac.clone().or(scanned_mac);
        if let Some(mac) = mac {
            if let Err(e) = self.run(&["bluetoothctl", "disconnect", &mac]) {
                eprintln!("phone-audio: warning: could not disconnect {mac}: {e}");
            }
        }
        Ok(())
    }

    /// status -> on ? turn_off : turn_on; returns the new `on` state.
    pub fn toggle(&mut self) -> Result<bool> {
        let on = self.status()?.on;
        if on {
            self.turn_off()?;
        } else {
            self.turn_on()?;
        }
        Ok(!on)
    }

    /// Drop the remembered phone (`last_seen`) from state, keeping the loopback
    /// pid untouched: after a phone is unpaired, a stale last_seen would keep
    /// reporting it as available forever. While off (no live loopback), status
    /// then reports no phone again until the next time it is seen.
    pub fn forget(&mut self) -> Result<()> {
        self.save_state(|s| s.last_seen = None)
    }

    /// Set volume 0..=100 on the phone's bluez Audio/Source node.
    pub fn set_volume(&mut self, pct: f64) -> Result<()> {
        let pct = pct.clamp(0.0, 100.0);
        let scan = self.scan()?;
        let Some(phone) = self.pick_phone(&scan).map(|d| &d.phone) else {
            return Err(AudioError::NotFound("no bluetooth phone connected".into()));
        };
        let Some(node) = find_source(&scan, &phone.mac) else {
            return Err(AudioError::NotFound(format!(
                "no bluez input source node for {} — start playback on the phone first",
                phone.name
            )));
        };
        self.run(&[
            "wpctl",
            "set-volume",
            &node.id.to_string(),
            &format!("{}", pct / 100.0),
        ])?;
        Ok(())
    }

    /// Persist a phone selection matched by MAC or name (both case-insensitive).
    pub fn set_phone(&mut self, name_or_mac: &str) -> Result<Phone> {
        let phones = self.list_phones()?;
        let needle = name_or_mac.to_uppercase();
        let found = phones
            .iter()
            .find(|p| p.mac == needle)
            .or_else(|| phones.iter().find(|p| p.name.to_uppercase() == needle))
            .cloned()
            .ok_or_else(|| {
                AudioError::NotFound(format!("no bluetooth phone matches '{name_or_mac}'"))
            })?;
        self.phone_mac = Some(found.mac.clone());
        self.save_config(&found.mac)?;
        Ok(found)
    }

    /// Multi-line troubleshooting dump.
    pub fn debug(&mut self) -> Result<String> {
        let mut o = String::new();
        o.push_str("=== phone-audio debug ===\n");
        let scan = match self.scan() {
            Ok(s) => s,
            Err(e) => {
                o.push_str(&format!("pw-dump failed: {e}\n"));
                Scan::default()
            }
        };
        for d in &scan.devices {
            let p = &d.phone;
            o.push_str(&format!(
                "phone: {} | mac={} | device={} | id={} | profile={}\n",
                p.name,
                p.mac,
                p.device_name,
                p.device_id,
                d.profile.as_deref().unwrap_or("-")
            ));
        }
        for n in &scan.sources {
            o.push_str(&format!("source node: {} (id={})\n", n.name, n.id));
        }
        for n in &scan.sinks {
            o.push_str(&format!("bluez sink node: {} (id={})\n", n.name, n.id));
        }
        match self.default_sink() {
            Ok(s) => o.push_str(&format!("default sink: {s}\n")),
            Err(e) => o.push_str(&format!("default sink: <error: {e}>\n")),
        }
        let pid = load_pid(&self.state_path);
        o.push_str(&format!(
            "state: loopback_pid={} alive={} last_seen={}\n",
            pid.map(|p| p.to_string()).unwrap_or_else(|| "-".into()),
            pid.is_some_and(pid_alive),
            load_last_seen(&self.state_path)
                .map(|l| l.mac)
                .unwrap_or_else(|| "-".into())
        ));
        o.push_str("(stale last_seen keeps a phone reported as available; clear it with 'phone-audio forget')\n");
        if let Some(dev) = self.pick_phone(&scan) {
            match self.run(&[
                "pw-cli",
                "enum-params",
                &dev.phone.device_id.to_string(),
                "EnumProfile",
            ]) {
                Ok(out) => {
                    o.push_str("enum-params tail:\n");
                    let lines: Vec<&str> = out.stdout.lines().collect();
                    for line in lines.into_iter().rev().take(15).rev() {
                        o.push_str(&format!("  {line}\n"));
                    }
                }
                Err(e) => o.push_str(&format!("enum-params failed: {e}\n")),
            }
        }
        Ok(o)
    }

    // ---- internals ---------------------------------------------------------

    /// Blocking run with a non-zero-exit check.
    fn run(&mut self, args: &[&str]) -> Result<CmdOut> {
        let out = self.runner.run(args)?;
        if out.status != 0 {
            let err = out.stderr.trim();
            let err = if err.is_empty() {
                format!("exit status {}", out.status)
            } else {
                err.to_string()
            };
            return Err(AudioError::Command {
                cmd: args[0].to_string(),
                err,
            });
        }
        Ok(out)
    }

    fn run_detached(&mut self, args: &[&str]) -> Result<u32> {
        self.runner.run_detached(args)
    }

    /// One `pw-dump` parse covering cards, source nodes and sink nodes.
    fn scan(&mut self) -> Result<Scan> {
        let out = self.run(&["pw-dump"])?;
        let root: Value = serde_json::from_str(&out.stdout).map_err(|e| AudioError::Command {
            cmd: "pw-dump".into(),
            err: format!("invalid JSON: {e}"),
        })?;
        let mut scan = Scan::default();
        let mut seen = HashSet::new();
        for v in root.as_array().into_iter().flatten() {
            let Some(p) = props(v) else { continue };
            if p.get("device.api").and_then(|x| x.as_str()) != Some("bluez5") {
                continue;
            }
            match obj_type(v) {
                Some("PipeWire:Interface:Device") => {
                    // Guard: a card without an address is not a usable remote phone.
                    let Some(mac) = p.get("api.bluez5.address").and_then(|x| x.as_str()) else {
                        continue;
                    };
                    if !seen.insert(mac.to_string()) {
                        continue;
                    }
                    let device_name = p
                        .get("device.name")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string();
                    let name = p
                        .get("device.alias")
                        .and_then(|x| x.as_str())
                        .or_else(|| p.get("device.description").and_then(|x| x.as_str()))
                        .unwrap_or(&device_name)
                        .to_string();
                    let profile = p
                        .get("bluez5.profile")
                        .and_then(|x| x.as_str())
                        .or_else(|| p.get("api.bluez5.profile").and_then(|x| x.as_str()))
                        .map(String::from);
                    let phone = Phone {
                        mac: mac.to_string(),
                        name,
                        device_name,
                        device_id: obj_id(v),
                    };
                    scan.devices.push(BlueDevice { phone, profile });
                }
                Some("PipeWire:Interface:Node") => {
                    let media = p.get("media.class").and_then(|x| x.as_str());
                    let node = BlueNode {
                        id: obj_id(v),
                        name: p
                            .get("node.name")
                            .and_then(|x| x.as_str())
                            .unwrap_or("")
                            .to_string(),
                    };
                    // PipeWire 1.6.x emits the bluez input node as
                    // Stream/Output/Audio (not Audio/Source), so the name prefix
                    // is the reliable signal; the class-based push stays for
                    // other configurations.
                    if node.name.starts_with("bluez_input.") || media == Some("Audio/Source") {
                        scan.sources.push(node);
                    } else if media == Some("Audio/Sink") {
                        scan.sinks.push(node);
                    }
                }
                _ => {}
            }
        }
        scan.devices.sort_by(|a, b| a.phone.name.cmp(&b.phone.name));
        scan.sources.sort_by(|a, b| a.name.cmp(&b.name));
        scan.sinks.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(scan)
    }

    fn pick_phone<'a>(&self, scan: &'a Scan) -> Option<&'a BlueDevice> {
        match &self.phone_mac {
            Some(mac) => scan.devices.iter().find(|d| d.phone.mac == *mac),
            None if scan.devices.len() == 1 => scan.devices.first(),
            None => None,
        }
    }

    /// The phone we know even when it is absent from the scan: the configured
    /// MAC (preferring a matching last_seen record for its display info), else
    /// the remembered last_seen phone. None when nothing is known at all.
    fn known_phone(&self) -> Option<Phone> {
        let last = load_last_seen(&self.state_path);
        match &self.phone_mac {
            Some(mac) => Some(
                last.filter(|p| p.mac == *mac)
                    .map(LastSeen::into_phone)
                    .unwrap_or_else(|| Phone {
                        mac: mac.clone(),
                        name: mac.clone(),
                        device_name: String::new(),
                        device_id: 0,
                    }),
            ),
            None => last.map(LastSeen::into_phone),
        }
    }

    /// Poll `pw-dump` up to [`WAIT_FOR_CARD_SECS`] for the configured phone's card.
    fn wait_for_card(&mut self, mac: &str) -> Result<()> {
        let polls = poll_count(WAIT_FOR_CARD_SECS);
        for i in 0..polls {
            let scan = self.scan()?;
            if scan.devices.iter().any(|d| d.phone.mac == *mac) {
                return Ok(());
            }
            if i + 1 < polls {
                std::thread::sleep(SOURCE_POLL_INTERVAL);
            }
        }
        Err(AudioError::NotFound(format!(
            "could not reconnect {mac} after {WAIT_FOR_CARD_SECS} s — is it powered on and in range?"
        )))
    }

    /// Poll `pw-dump` up to [`WAIT_FOR_SOURCE_SECS`] for the
    /// `bluez_input.<mac>.*` source node, announcing the wait once when it
    /// actually has to block on the phone starting to stream.
    fn wait_for_source(&mut self, mac: &str) -> Result<Option<String>> {
        let polls = poll_count(WAIT_FOR_SOURCE_SECS);
        for i in 0..polls {
            let scan = self.scan()?;
            if let Some(node) = find_source(&scan, mac) {
                return Ok(Some(node.name.clone()));
            }
            if i == 0 {
                eprintln!("waiting for the phone to play (up to {WAIT_FOR_SOURCE_SECS} s)…");
            }
            if i + 1 < polls {
                std::thread::sleep(SOURCE_POLL_INTERVAL);
            }
        }
        Ok(None)
    }

    /// `pactl get-default-sink`, falling back to parsing `wpctl status` for the `*` row.
    fn default_sink(&mut self) -> Result<String> {
        if let Ok(out) = self.run(&["pactl", "get-default-sink"]) {
            let name = out.stdout.trim();
            if !name.is_empty() {
                return Ok(name.to_string());
            }
        }
        let out = self.run(&["wpctl", "status"])?;
        for line in out.stdout.lines() {
            if !line.trim_start().starts_with('*') {
                continue;
            }
            let mut it = line.split_whitespace();
            while let Some(tok) = it.next() {
                if tok.ends_with('.') && tok[..tok.len() - 1].parse::<u32>().is_ok() {
                    if let Some(name) = it.next() {
                        return Ok(name.to_string());
                    }
                }
            }
        }
        Err(AudioError::NotFound(
            "could not determine default sink".into(),
        ))
    }

    fn save_config(&self, mac: &str) -> Result<()> {
        self.save_json(
            &self.cfg_path,
            &ConfigFile {
                phone_mac: Some(mac.to_string()),
            },
        )
    }

    fn save_state_pid(&self, pid: u32, node: &str) -> Result<()> {
        self.save_state(|s| {
            s.loopback_pid = Some(pid);
            s.loopback_node = Some(node.to_string());
        })
    }

    fn save_last_seen(&self, phone: &Phone) -> Result<()> {
        self.save_state(|s| {
            s.last_seen = Some(LastSeen {
                mac: phone.mac.clone(),
                name: phone.name.clone(),
                device_name: phone.device_name.clone(),
            })
        })
    }

    /// Like [`Self::save_last_seen`], but skips the write when the stored
    /// record already matches: `status()` is polled once a second and must not
    /// rewrite state.json on every poll (a no-op rewrite would race the toggle
    /// worker's own rename on the same tmp file for zero benefit).
    fn save_last_seen_if_changed(&self, phone: &Phone) -> Result<()> {
        let same = load_last_seen(&self.state_path)
            .map(|last| {
                last.mac == phone.mac
                    && last.name == phone.name
                    && last.device_name == phone.device_name
            })
            .unwrap_or(false);
        if same {
            Ok(())
        } else {
            self.save_last_seen(phone)
        }
    }

    /// Load-modify-save on state.json, preserving fields other than the one
    /// being updated (a pid write must not drop the remembered phone and vice
    /// versa).
    fn save_state(&self, update: impl FnOnce(&mut StateFile)) -> Result<()> {
        let mut state = load_state(&self.state_path);
        update(&mut state);
        self.save_json(&self.state_path, &state)
    }

    fn save_json<T: Serialize>(&self, path: &Path, val: &T) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = tmp_path(path);
        std::fs::write(&tmp, serde_json::to_string_pretty(val)?)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    /// Drop the loopback pid but keep `last_seen`: a disconnected-but-known
    /// phone must stay available so the toggle can reconnect it after `off`.
    /// Without a last_seen the file is simply removed (nothing left to keep).
    fn clear_state(&self) -> Result<()> {
        if let Some(last) = load_last_seen(&self.state_path) {
            return self.save_json(
                &self.state_path,
                &StateFile {
                    loopback_pid: None,
                    loopback_node: None,
                    last_seen: Some(last),
                },
            );
        }
        match std::fs::remove_file(&self.state_path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

fn config_dir() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".into())).join(".config")
        });
    base.join("phone-audio")
}

/// A tmp path unique per writer per call: the GUI's 1/sec status poll and a
/// toggle worker both write state.json from the same process, so a shared
/// tmp name would let one writer's rename clobber the other's in-flight bytes
/// (lost update -> dropped loopback pid -> `on` later spawns a duplicate
/// pw-loopback). The rename-to-final stays atomic; each writer's bytes are
/// complete before its own rename.
fn tmp_path(path: &Path) -> PathBuf {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut tmp = path.as_os_str().to_os_string();
    tmp.push(format!(".tmp.{}.{seq}", std::process::id()));
    PathBuf::from(tmp)
}

/// Missing or malformed file -> None (no selection; next set-phone rewrites it).
fn load_config(path: &Path) -> Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(s) => Ok(serde_json::from_str::<ConfigFile>(&s)
            .ok()
            .and_then(|c| c.phone_mac)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// State is disposable: any read problem just means "no state".
fn load_state(path: &Path) -> StateFile {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str::<StateFile>(&s).ok())
        .unwrap_or_default()
}

/// Missing or malformed state -> no loopback pid.
fn load_pid(path: &Path) -> Option<u32> {
    load_state(path).loopback_pid
}

/// Missing or malformed state -> no remembered phone.
fn load_last_seen(path: &Path) -> Option<LastSeen> {
    load_state(path).last_seen
}

/// Process-alive check; Linux-only per spec (`/proc/<pid>`).
fn pid_alive(pid: u32) -> bool {
    // Identity: must actually be our loopback.
    let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).unwrap_or_default();
    if comm.trim() != "pw-loopback" {
        return false;
    }
    // /proc/<pid>/stat field 3 = process state char; 'Z' = zombie (exited, unreaped).
    std::fs::read_to_string(format!("/proc/{pid}/stat"))
        .ok()
        .and_then(|s| s.split_whitespace().nth(2).map(str::to_string))
        .map(|st| st != "Z")
        .unwrap_or(false)
}

fn find_source<'a>(scan: &'a Scan, mac: &str) -> Option<&'a BlueNode> {
    let prefix = format!("bluez_input.{}.", mac_underscored(mac));
    // Prefer the real bluez input node over its `.monitor` sibling.
    scan.sources
        .iter()
        .filter(|n| n.name.starts_with(&prefix))
        .min_by_key(|n| if n.name.ends_with(".monitor") { 1 } else { 0 })
}

/// The on/off decision: our loopback pid is alive `&&` the phone's bluez input
/// source node exists (profile is idle-state info and can't carry this).
fn is_on(pid_alive: bool, source: bool) -> bool {
    pid_alive && source
}

#[cfg(test)]
mod tests; // tests live in tests.rs to keep the fixture JSON out of this file's noise
