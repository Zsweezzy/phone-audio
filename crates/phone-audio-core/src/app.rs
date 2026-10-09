//! The application: discovery via `pw-dump`, profile switching, loopback, config/state.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::cmd::{CmdOut, CmdRunner, RealRunner};
use crate::pw::{
    mac_underscored, obj_id, obj_type, parse_profiles, parse_volume, pick_receive_profile, props,
    RECEIVE_PROFILES,
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

#[derive(Debug, Serialize, Deserialize)]
struct StateFile {
    #[serde(default)]
    loopback_pid: Option<u32>,
}

/// The app. Holds its configuration/state paths and the command runner.
pub struct App {
    runner: Box<dyn CmdRunner>,
    cfg_path: PathBuf,
    state_path: PathBuf,
    phone_mac: Option<String>,
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
            return Ok(Status {
                available: false,
                reason: NO_PHONE_REASON.into(),
                phones,
                ..Default::default()
            });
        }
        let Some(dev) = self.pick_phone(&scan) else {
            let reason = if self.phone_mac.is_some() {
                "configured phone not connected (run 'phone-audio set-phone' again)".into()
            } else {
                MULTI_PHONE_REASON.into()
            };
            return Ok(Status {
                available: true,
                reason,
                phones,
                ..Default::default()
            });
        };
        let profile = dev.profile.clone();
        let pid = load_pid(&self.state_path);
        let pid_alive = pid.is_some_and(pid_alive);
        let on = pid_alive
            && profile
                .as_deref()
                .is_some_and(|p| RECEIVE_PROFILES.contains(&p));
        let source = find_source(&scan, &dev.phone.mac);
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
        } else if source.is_none() {
            "not streaming — start playback on the phone".into()
        } else if profile.as_deref() == Some("off") {
            "profile off — run 'phone-audio on'".into()
        } else {
            String::new()
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
        let scan = self.scan()?;
        let Some(dev) = self.pick_phone(&scan) else {
            return Err(AudioError::NotFound("no bluetooth phone connected".into()));
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

        if load_pid(&self.state_path).is_some_and(pid_alive) {
            return Ok(()); // already on
        }
        let sink = self.default_sink()?;
        let pid = self.run_detached(&["pw-loopback", "-C", &node_name, "-P", &sink])?;
        self.save_state_pid(pid)
    }

    /// Kill our loopback and drop the profile so the phone plays on its own speaker.
    pub fn turn_off(&mut self) -> Result<()> {
        if let Some(pid) = load_pid(&self.state_path) {
            if pid_alive(pid) {
                self.run(&["kill", &pid.to_string()])?;
            }
            self.clear_state()?;
        }
        if let Ok(scan) = self.scan() {
            if let Some(dev) = self.pick_phone(&scan) {
                // Non-fatal: switching to "off" when it already is off can be non-zero,
                // but the user must know if the drop actually failed.
                if let Err(e) =
                    self.run(&["pactl", "set-card-profile", &dev.phone.device_name, "off"])
                {
                    eprintln!("phone-audio: warning: could not drop profile: {e}");
                }
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
            "state: loopback_pid={} alive={}\n",
            pid.map(|p| p.to_string()).unwrap_or_else(|| "-".into()),
            pid.is_some_and(pid_alive)
        ));
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
                    match media {
                        Some("Audio/Source") => scan.sources.push(node),
                        Some("Audio/Sink") => scan.sinks.push(node),
                        _ => {}
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

    /// Poll `pw-dump` up to ~2.5s for the `bluez_input.<mac>.*` source node.
    fn wait_for_source(&mut self, mac: &str) -> Result<Option<String>> {
        for i in 0..13 {
            let scan = self.scan()?;
            if let Some(node) = find_source(&scan, mac) {
                return Ok(Some(node.name.clone()));
            }
            if i < 12 {
                std::thread::sleep(Duration::from_millis(200));
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

    fn save_state_pid(&self, pid: u32) -> Result<()> {
        self.save_json(
            &self.state_path,
            &StateFile {
                loopback_pid: Some(pid),
            },
        )
    }

    fn save_json<T: Serialize>(&self, path: &Path, val: &T) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(val)?)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    fn clear_state(&self) -> Result<()> {
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

/// State is disposable: any read problem just means "no loopback pid".
fn load_pid(path: &Path) -> Option<u32> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str::<StateFile>(&s).ok())
        .and_then(|s| s.loopback_pid)
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

#[cfg(test)]
mod tests; // tests live in tests.rs to keep the fixture JSON out of this file's noise
