//! Behavior tests for [`super::App`] against a scripted [`FakeRunner`].
//! These never touch the real machine: no phone, no PipeWire required.

use std::collections::VecDeque;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use super::*;
use crate::cmd::CmdOut;
use crate::Result;

/// Call log shared between tests and the boxed runner: the runner is `Send`,
/// but the log lives on `Arc<Mutex>` so tests can read it back after boxing.
type Detached = Vec<(Vec<String>, u32)>;

#[derive(Clone, Default)]
struct Log {
    calls: Arc<Mutex<Vec<Vec<String>>>>,
    detached: Arc<Mutex<Detached>>,
}

impl Log {
    fn calls(&self) -> Vec<Vec<String>> {
        self.calls.lock().unwrap().clone()
    }

    fn detached(&self) -> Vec<(Vec<String>, u32)> {
        self.detached.lock().unwrap().clone()
    }
}

/// Records argv and returns scripted responses; empty queue -> empty OK.
#[derive(Clone)]
struct FakeRunner {
    queue: VecDeque<CmdOut>,
    log: Log,
    next_pid: u32,
}

impl FakeRunner {
    fn new(responses: Vec<CmdOut>) -> Self {
        Self {
            queue: responses.into(),
            log: Log::default(),
            next_pid: 1000,
        }
    }

    fn ok(stdout: &str) -> CmdOut {
        CmdOut {
            status: 0,
            stdout: stdout.into(),
            stderr: String::new(),
        }
    }

    /// Clone of the shared call log; call before boxing into an `App`.
    fn log(&self) -> Log {
        self.log.clone()
    }
}

impl CmdRunner for FakeRunner {
    fn run(&mut self, args: &[&str]) -> Result<CmdOut> {
        self.log
            .calls
            .lock()
            .unwrap()
            .push(args.iter().map(|s| s.to_string()).collect());
        Ok(self.queue.pop_front().unwrap_or_default())
    }

    fn run_detached(&mut self, args: &[&str]) -> Result<u32> {
        let pid = self.next_pid;
        self.next_pid += 1;
        self.log
            .detached
            .lock()
            .unwrap()
            .push((args.iter().map(|s| s.to_string()).collect(), pid));
        Ok(pid)
    }

    fn box_clone(&self) -> Box<dyn CmdRunner> {
        Box::new(self.clone())
    }
}

const MAC: &str = "28:8F:F6:71:6F:6E";
const DEVICE_NAME: &str = "bluez_card.28_8F_F6_71_6F_6E";
const DEVICE_ID: u32 = 45;
const SOURCE_NODE: &str = "bluez_input.28_8F_F6_71_6F_6E.1";
/// The node name after the phone's stream restarts once (`.1` -> `.2`): the
/// stale-loopback scenario — the recorded loopback is alive but pointed at the
/// old name while state still claims it is current.
const SOURCE_NODE_2: &str = "bluez_input.28_8F_F6_71_6F_6E.2";
const SINK: &str = "alsa_output.usb-GeneralPlus_USB_Audio_Device-00.analog-stereo";

fn bluez_device(profile: &str) -> serde_json::Value {
    serde_json::json!({
        "id": DEVICE_ID,
        "type": "PipeWire:Interface:Device",
        "info": { "props": {
            "device.api": "bluez5",
            "device.name": DEVICE_NAME,
            "device.alias": "Maxii",
            "api.bluez5.address": MAC,
            "api.bluez5.profile": profile,
        }}
    })
}

fn other_device() -> serde_json::Value {
    serde_json::json!({
        "id": 99,
        "type": "PipeWire:Interface:Device",
        "info": { "props": {
            "device.api": "bluez5",
            "device.name": "bluez_card.AA_BB_CC_DD_EE_FF",
            "device.alias": "OldIphone",
            "api.bluez5.address": "AA:BB:CC:DD:EE:FF",
            "api.bluez5.profile": "off",
        }}
    })
}

fn source_node() -> serde_json::Value {
    source_node_named(SOURCE_NODE)
}

fn source_node_named(name: &str) -> serde_json::Value {
    // Real pipewire 1.6.9 emits the bluez input node as Stream/Output/Audio,
    // not Audio/Source — the app must find it by node.name, not media.class.
    serde_json::json!({
        "id": 51,
        "type": "PipeWire:Interface:Node",
        "info": { "props": {
            "media.class": "Stream/Output/Audio",
            "device.api": "bluez5",
            "node.name": name,
            "node.description": "Maxii",
        }}
    })
}

fn alsa_sink() -> serde_json::Value {
    serde_json::json!({
        "id": 60,
        "type": "PipeWire:Interface:Node",
        "info": { "props": {
            "media.class": "Audio/Sink",
            "node.name": SINK,
        }}
    })
}

fn dump_json(devices: &[serde_json::Value], nodes: &[serde_json::Value]) -> CmdOut {
    let mut arr: Vec<serde_json::Value> = devices.to_vec();
    arr.extend_from_slice(nodes);
    FakeRunner::ok(&serde_json::to_string(&arr).unwrap())
}

fn one_phone_dump(profile: &str) -> CmdOut {
    dump_json(&[bluez_device(profile)], &[source_node(), alsa_sink()])
}

fn enum_profiles_text() -> &'static str {
    r#"  Object: size 160, type Spa:Pod:Object:Param:Profile (262151), id Spa:Enum:ParamId:EnumProfile (8)
    Prop: key Spa:Pod:Object:Param:Profile:index (1), flags 00000000
      Int 0
    Prop: key Spa:Pod:Object:Param:Profile:name (2), flags 00000000
      String "off"
    Prop: key Spa:Pod:Object:Param:Profile:description (3), flags 00000000
      String "Off"
    Prop: key Spa:Pod:Object:Param:Profile:available (5), flags 00000000
      Id 1        (Spa:Enum:ParamAvailability:no)
    Prop: key Spa:Pod:Object:Param:Profile:classes (7), flags 00000000
      Struct: size 224
        Int 1
        Struct: size 96
          String "Audio/Sink"
          Int 1
  Object: size 160, type Spa:Pod:Object:Param:Profile (262151), id Spa:Enum:ParamId:EnumProfile (8)
    Prop: key Spa:Pod:Object:Param:Profile:index (1), flags 00000000
      Int 1
    Prop: key Spa:Pod:Object:Param:Profile:name (2), flags 00000000
      String "a2dp-sink"
    Prop: key Spa:Pod:Object:Param:Profile:description (3), flags 00000000
      String "High Fidelity Playback (A2DP Sink)"
    Prop: key Spa:Pod:Object:Param:Profile:available (5), flags 00000000
      Id 2        (Spa:Enum:ParamAvailability:yes)
    Prop: key Spa:Pod:Object:Param:Profile:classes (7), flags 00000000
      Struct: size 224
        Int 1
        Struct: size 96
          String "Audio/Sink"
          Int 1
  Object: size 160, type Spa:Pod:Object:Param:Profile (262151), id Spa:Enum:ParamId:EnumProfile (8)
    Prop: key Spa:Pod:Object:Param:Profile:index (1), flags 00000000
      Int 2
    Prop: key Spa:Pod:Object:Param:Profile:name (2), flags 00000000
      String "a2dp-source"
    Prop: key Spa:Pod:Object:Param:Profile:description (3), flags 00000000
      String "High Fidelity Capture (A2DP Source)"
    Prop: key Spa:Pod:Object:Param:Profile:available (5), flags 00000000
      Id 2        (Spa:Enum:ParamAvailability:yes)
    Prop: key Spa:Pod:Object:Param:Profile:classes (7), flags 00000000
      Struct: size 224
        Int 1
        Struct: size 96
          String "Audio/Source"
          Int 1
"#
}

/// The real bluez card (PipeWire 1.6.9 / WirePlumber 0.5.18): only `off` and
/// `audio-gateway`, with NO `classes` struct in either block.
fn enum_classless_text() -> &'static str {
    r#"  Object: size 160, type Spa:Pod:Object:Param:Profile (262151), id Spa:Enum:ParamId:EnumProfile (8)
    Prop: key Spa:Pod:Object:Param:Profile:index (1), flags 00000000
      Int 0
    Prop: key Spa:Pod:Object:Param:Profile:name (2), flags 00000000
      String "off"
    Prop: key Spa:Pod:Object:Param:Profile:description (3), flags 00000000
      String "Off"
    Prop: key Spa:Pod:Object:Param:Profile:available (5), flags 00000000
      Id 2        (Spa:Enum:ParamAvailability:yes)
  Object: size 160, type Spa:Pod:Object:Param:Profile (262151), id Spa:Enum:ParamId:EnumProfile (8)
    Prop: key Spa:Pod:Object:Param:Profile:index (1), flags 00000000
      Int 1
    Prop: key Spa:Pod:Object:Param:Profile:name (2), flags 00000000
      String "audio-gateway"
    Prop: key Spa:Pod:Object:Param:Profile:description (3), flags 00000000
      String "Audio Gateway (A2DP Source & HSP/HFP AG)"
    Prop: key Spa:Pod:Object:Param:Profile:available (5), flags 00000000
      Id 2        (Spa:Enum:ParamAvailability:yes)
"#
}

/// Unique per process + per call: the spawn-based tests leave short-lived
/// renamed binaries that outlive a panicking test, so a reused PID (or a
/// coarse clock) across back-to-back `cargo test` runs can otherwise make two
/// runs collide on one path — the later copy/exec then hits a still-running
/// orphan's executable (ETXTBSY).
static TMP_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn tmp_dir(name: &str) -> PathBuf {
    let start = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let seq = TMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let d = std::env::temp_dir().join(format!("pa-test-{name}-{start}-{seq}"));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// ETXTBSY (os error 26): a `sleep 30` orphan from a crashed test run can still
/// be executing its renamed image, which makes copy/spawn of that path fail.
/// Bounded retry — the orphan exits within 30s.
fn retry_busy<T>(mut f: impl FnMut() -> std::io::Result<T>, what: &str) -> T {
    let mut stuck: Option<String> = None;
    for _ in 0..50 {
        match f() {
            Ok(v) => return v,
            Err(e) if e.kind() == std::io::ErrorKind::ExecutableFileBusy => {
                std::thread::sleep(std::time::Duration::from_millis(10));
                stuck = Some(e.to_string());
            }
            Err(e) => panic!("{what} failed: {e}"),
        }
    }
    panic!("{what} still busy after retries ({stuck:?})")
}

/// Launch a fake `pw-loopback`: a copy of `sleep` renamed to the real binary
/// name so pid_alive()'s comm check passes. Returns once /proc shows the name.
fn spawn_loopback(exe: &Path) -> std::process::Child {
    let sleep = ["/bin/sleep", "/usr/bin/sleep"]
        .into_iter()
        .find(|p| Path::new(p).exists())
        .expect("no sleep binary to copy");
    retry_busy(
        || std::fs::copy(sleep, exe).map(|_| ()),
        &format!("copy {sleep} -> {}", exe.display()),
    );
    let child = retry_busy(
        || std::process::Command::new(exe).arg("30").spawn(),
        &format!("spawn {}", exe.display()),
    );
    // exec runs just after spawn; wait until /proc reports the chosen name.
    for _ in 0..100 {
        if std::fs::read_to_string(format!("/proc/{}/comm", child.id()))
            .map(|s| s.trim() == "pw-loopback")
            .unwrap_or(false)
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    child
}

fn seed_state_pid(state: &Path) -> u32 {
    // A pid that is *not* a live pw-loopback (identity check in pid_alive):
    // this test process's /proc entry exists but its comm is not "pw-loopback",
    // exactly the stale/zombie-pid situation fix 1 protects against.
    let pid = std::process::id();
    std::fs::write(state, format!("{{\"loopback_pid\": {pid}}}")).unwrap();
    pid
}

/// State with a REAL live pw-loopback process (so `pid_alive` is true) and an
/// optional stored capture node; returns the child so the test can reap it.
fn seed_live_state(state: &Path, node: Option<&str>) -> std::process::Child {
    let child = spawn_loopback(&state.parent().unwrap().join("pw-loopback"));
    let pid = child.id();
    let node = node
        .map(|n| format!(", \"loopback_node\": \"{n}\""))
        .unwrap_or_default();
    std::fs::write(state, format!("{{\"loopback_pid\": {pid}{node}}}")).unwrap();
    child
}

fn default_sink_out() -> CmdOut {
    FakeRunner::ok(&format!("{SINK}\n"))
}

/// Inode of a file (0 when absent): save_json replaces the final file via
/// rename, so a rewrite changes the inode and a non-write keeps it. A
/// granularity-free "was state.json rewritten?" probe for the W1 test.
fn file_ino(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.ino()).unwrap_or(0)
}

// ---- list / status -------------------------------------------------------

#[test]
fn list_phones_finds_maxii_from_dump() {
    let fake = FakeRunner::new(vec![one_phone_dump("a2dp-source")]);
    let mut app = App::with_runner(Box::new(fake), None);
    let phones = app.list_phones().unwrap();
    assert_eq!(phones.len(), 1);
    assert_eq!(
        phones[0],
        Phone {
            mac: MAC.into(),
            name: "Maxii".into(),
            device_name: DEVICE_NAME.into(),
            device_id: DEVICE_ID,
        }
    );
}

#[test]
fn status_empty_scan_keeps_configured_phone_available() {
    // Configured MAC but no device in the scan (also the post-`off` state):
    // the phone stays available so the toggle stays enabled, the reason points
    // at the reconnect flow, and phones lists the known phone.
    let dir = tmp_dir("status-known");
    let mut app = App::with_paths(
        Box::new(FakeRunner::new(vec![dump_json(&[], &[alsa_sink()])])),
        dir.join("config.json"),
        dir.join("state.json"),
        Some(MAC.into()),
    );
    let s = app.status().unwrap();
    assert!(
        s.available,
        "known-but-disconnected phone must stay available"
    );
    assert!(!s.on);
    assert!(s.profile.is_none());
    assert_eq!(s.phones.len(), 1);
    assert_eq!(s.phones[0].mac, MAC);
    assert_eq!(
        s.phone.as_ref().unwrap().mac,
        MAC,
        "known phone also in `phone`"
    );
    assert_eq!(s.reason, RECONNECT_REASON);
}

#[test]
fn status_empty_scan_uses_last_seen_phone_when_unconfigured() {
    // No configured MAC, but a phone was seen before and remembered in state:
    // it stays available (reconnect reason) with last_seen as the known phone.
    let dir = tmp_dir("status-last-seen");
    let state = dir.join("state.json");
    std::fs::write(
        &state,
        format!(
            "{{\"last_seen\": {{\"mac\": \"{MAC}\", \"name\": \"Maxii\", \"device_name\": \"{DEVICE_NAME}\"}}}}"
        ),
    )
    .unwrap();
    let mut app = App::with_paths(
        Box::new(FakeRunner::new(vec![dump_json(&[], &[alsa_sink()])])),
        dir.join("config.json"),
        state,
        None,
    );
    let s = app.status().unwrap();
    assert!(
        s.available,
        "remembered-but-disconnected phone stays available"
    );
    assert!(!s.on);
    assert!(s.profile.is_none());
    assert_eq!(
        s.phones,
        vec![Phone {
            mac: MAC.into(),
            name: "Maxii".into(),
            device_name: DEVICE_NAME.into(),
            device_id: 0,
        }]
    );
    assert_eq!(
        s.phone.as_ref().unwrap().mac,
        MAC,
        "known phone also in `phone`"
    );
    assert_eq!(s.reason, RECONNECT_REASON);
}

#[test]
fn status_with_no_phone_is_unavailable() {
    let dir = tmp_dir("status-none");
    let mut app = App::with_paths(
        Box::new(FakeRunner::new(vec![dump_json(&[], &[alsa_sink()])])),
        dir.join("config.json"),
        dir.join("state.json"),
        None,
    );
    let s = app.status().unwrap();
    assert!(!s.available);
    assert!(s.phone.is_none());
    assert!(!s.on);
    assert!(s.reason.contains("no bluetooth phone"));
}

#[test]
fn status_with_two_phones_needs_phone_picked() {
    let dir = tmp_dir("status-two");
    let mut app = App::with_paths(
        Box::new(FakeRunner::new(vec![dump_json(
            &[bluez_device("off"), other_device()],
            &[],
        )])),
        dir.join("config.json"),
        dir.join("state.json"),
        None,
    );
    let s = app.status().unwrap();
    assert!(s.available);
    assert!(s.phone.is_none());
    assert!(s.reason.contains("set-phone"));
}

#[test]
fn status_with_single_phone_reports_profile_and_volume() {
    let dir = tmp_dir("status-one");
    let mut app = App::with_paths(
        Box::new(FakeRunner::new(vec![
            one_phone_dump("a2dp-source"),
            FakeRunner::ok("Volume: 0.65\n"),
        ])),
        dir.join("config.json"),
        dir.join("state.json"),
        None,
    );
    let s = app.status().unwrap();
    assert!(s.available);
    assert_eq!(s.phone.as_ref().unwrap().name, "Maxii");
    assert!(!s.on);
    assert_eq!(s.profile.as_deref(), Some("a2dp-source"));
    assert_eq!(s.volume, Some(65.0));
}

#[test]
fn status_prefers_bluez5_profile_key_over_api_fallback() {
    // Real pw-dump emits `bluez5.profile` (not `api.bluez5.profile`); when both
    // keys are present the unprefixed one wins.
    let mut dev = bluez_device("off");
    dev["info"]["props"]["bluez5.profile"] = serde_json::json!("audio-gateway");
    let fake = FakeRunner::new(vec![
        dump_json(&[dev], &[source_node(), alsa_sink()]),
        FakeRunner::ok("Volume: 0.5\n"),
    ]);
    let dir = tmp_dir("status-profile");
    let mut app = App::with_paths(
        Box::new(fake),
        dir.join("config.json"),
        dir.join("state.json"),
        None,
    );
    let s = app.status().unwrap();
    assert_eq!(s.profile.as_deref(), Some("audio-gateway"));
}

#[test]
fn status_persists_last_seen_phone_when_device_present() {
    // Every status with a device present remembers it in state.json, so a
    // later `off` + empty scan still knows which phone to reconnect.
    let dir = tmp_dir("last-seen");
    let state = dir.join("state.json");
    let mut app = App::with_paths(
        Box::new(FakeRunner::new(vec![
            one_phone_dump("a2dp-source"),
            FakeRunner::ok("Volume: 0.5\n"),
        ])),
        dir.join("config.json"),
        state.clone(),
        None,
    );
    app.status().unwrap();
    let stored: StateFile =
        serde_json::from_str(&std::fs::read_to_string(&state).unwrap()).unwrap();
    let last = stored.last_seen.expect("last_seen persisted");
    assert_eq!(last.mac, MAC);
    assert_eq!(last.name, "Maxii");
    assert_eq!(last.device_name, DEVICE_NAME);
}

#[test]
fn status_does_not_rewrite_state_when_phone_unchanged() {
    // W1: the GUI polls status once a second while a toggle worker may be
    // writing the loopback pid. status must not rewrite state.json when the
    // remembered phone is unchanged — a same-content rewrite is pure race
    // surface. save_json replaces the file via rename, so a rewrite shows as
    // a new inode; no write keeps the same inode.
    let dir = tmp_dir("status-no-rewrite");
    let state = dir.join("state.json");
    let fake = FakeRunner::new(vec![
        one_phone_dump("a2dp-source"),
        FakeRunner::ok("Volume: 0.5\n"),
        one_phone_dump("a2dp-source"),
        FakeRunner::ok("Volume: 0.5\n"),
        dump_json(&[other_device()], &[source_node(), alsa_sink()]),
    ]);
    let log = fake.log();
    let mut app = App::with_paths(Box::new(fake), dir.join("config.json"), state.clone(), None);

    app.status().unwrap(); // first poll persists last_seen
    let ino_first = file_ino(&state);
    let bytes = std::fs::read(&state).unwrap();

    app.status().unwrap(); // same phone again -> must not rewrite
    assert_eq!(
        file_ino(&state),
        ino_first,
        "polling status with an unchanged phone must not rewrite state.json"
    );
    assert_eq!(std::fs::read(&state).unwrap(), bytes);

    // Control: a different phone in the scan DOES rewrite (the mechanism is
    // live, it just skips no-op writes).
    app.status().unwrap();
    assert_ne!(
        file_ino(&state),
        ino_first,
        "a changed phone must rewrite last_seen"
    );
    // 3 scans total; the AA:BB phone has no bluez_input node, so no volume call.
    assert_eq!(log.calls().len(), 5);
}

#[test]
fn status_streaming_without_loopback_is_off_with_on_reason() {
    // Real state (pipewire 1.6.9): bluez5.profile stays "off" while the phone
    // streams and the bluez_input node exists. `on` must reflect the running
    // loopback, not the profile field.
    let fake = FakeRunner::new(vec![one_phone_dump("off"), FakeRunner::ok("Volume: 0.5\n")]);
    let dir = tmp_dir("status-stream");
    let mut app = App::with_paths(
        Box::new(fake),
        dir.join("config.json"),
        dir.join("state.json"),
        None,
    );
    let s = app.status().unwrap();
    assert!(s.available);
    assert!(!s.on);
    assert_eq!(s.profile.as_deref(), Some("off"));
    assert_eq!(s.reason, "streaming — run 'phone-audio on'");
    assert_eq!(s.volume, Some(50.0));
}

#[test]
fn status_profile_off_without_source_reports_profile_off_reason() {
    let dir = tmp_dir("status-profile-off");
    let mut app = App::with_paths(
        Box::new(FakeRunner::new(vec![dump_json(
            &[bluez_device("off")],
            &[alsa_sink()],
        )])),
        dir.join("config.json"),
        dir.join("state.json"),
        None,
    );
    let s = app.status().unwrap();
    assert!(!s.on);
    assert_eq!(s.reason, "profile off — run 'phone-audio on'");
}

#[test]
fn status_non_receive_profile_without_source_reports_not_streaming_reason() {
    let dir = tmp_dir("status-no-source");
    let mut app = App::with_paths(
        Box::new(FakeRunner::new(vec![dump_json(
            &[bluez_device("a2dp-sink")],
            &[alsa_sink()],
        )])),
        dir.join("config.json"),
        dir.join("state.json"),
        None,
    );
    let s = app.status().unwrap();
    assert!(!s.on);
    assert_eq!(s.reason, "not streaming — start playback on the phone");
}

#[test]
fn is_on_requires_live_loopback_pid_and_present_source() {
    // Pure on-decision truth table (status() delegates to this).
    assert!(is_on(true, true));
    assert!(!is_on(true, false));
    assert!(!is_on(false, true));
    assert!(!is_on(false, false));
}

#[test]
fn status_on_true_with_live_loopback_pid_and_present_source_node() {
    // status() reads /proc/<pid> and demands comm == "pw-loopback", so hand it
    // a real live process with that name: a copy of `sleep` renamed. The on
    // decision must not depend on the profile (which stays "off" while live).
    let dir = tmp_dir("status-on");
    let state = dir.join("state.json");
    let exe = dir.join("pw-loopback");
    let mut child = spawn_loopback(&exe);
    std::fs::write(
        &state,
        format!(
            "{{\"loopback_pid\": {}, \"loopback_node\": \"{SOURCE_NODE}\"}}",
            child.id()
        ),
    )
    .unwrap();

    let fake = FakeRunner::new(vec![one_phone_dump("off"), FakeRunner::ok("Volume: 0.5\n")]);
    let mut app = App::with_paths(Box::new(fake), dir.join("config.json"), state, None);
    let s = app.status().unwrap();
    assert!(s.on);
    assert_eq!(s.reason, "");
    assert_eq!(s.profile.as_deref(), Some("off"));
    assert_eq!(s.volume, Some(50.0));

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn status_reports_off_when_live_loopback_node_does_not_match_source() {
    // Alive pid + source present, but the recorded node is the old name (the
    // stream-restart scenario): not on. The existing reason branch points the
    // user at the one re-toggle that kills + re-arms with the current node.
    let dir = tmp_dir("status-node-mismatch");
    let state = dir.join("state.json");
    let mut child = seed_live_state(&state, Some(SOURCE_NODE)); // recorded: .1
    let fake = FakeRunner::new(vec![
        dump_json(
            &[bluez_device("off")],
            &[source_node_named(SOURCE_NODE_2), alsa_sink()],
        ), // current: .2
        FakeRunner::ok("Volume: 0.5\n"),
    ]);
    let mut app = App::with_paths(Box::new(fake), dir.join("config.json"), state, None);
    let s = app.status().unwrap();
    assert!(s.available);
    assert!(!s.on, "stale loopback on a dead node must read off");
    assert_eq!(s.reason, "streaming — run 'phone-audio on'");
    assert_eq!(s.volume, Some(50.0));

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn status_reports_off_for_legacy_state_without_stored_node() {
    // State files written before the node field existed have only loopback_pid:
    // no stored node -> node_matches is false -> off (one re-toggle re-arms it
    // with the current node, permanently self-healing).
    let dir = tmp_dir("status-legacy-node");
    let state = dir.join("state.json");
    let mut child = seed_live_state(&state, None);
    let fake = FakeRunner::new(vec![one_phone_dump("off"), FakeRunner::ok("Volume: 0.5\n")]);
    let mut app = App::with_paths(Box::new(fake), dir.join("config.json"), state, None);
    let s = app.status().unwrap();
    assert!(!s.on, "legacy state without a node must read off");
    assert_eq!(s.reason, "streaming — run 'phone-audio on'");

    let _ = child.kill();
    let _ = child.wait();
}

// ---- turn_on -------------------------------------------------------------

#[test]
fn turn_on_switches_profile_then_loops_back_and_persists_pid() {
    let dir = tmp_dir("turn-on");
    let cfg = dir.join("config.json");
    let state = dir.join("state.json");
    let responses = vec![
        one_phone_dump("off"),                // phone discovery
        FakeRunner::ok(enum_profiles_text()), // pick a2dp-source
        FakeRunner::ok(""),                   // pactl set-card-profile a2dp-source
        one_phone_dump("a2dp-source"),        // poll finds bluez_input node
        default_sink_out(),                   // pactl get-default-sink
    ];
    let fake = FakeRunner::new(responses);
    let log = fake.log();
    let mut app = App::with_paths(Box::new(fake), cfg.clone(), state.clone(), None);
    app.turn_on().unwrap();

    let calls = log.calls();
    assert_eq!(calls[0], ["pw-dump"]);
    assert_eq!(
        calls[1],
        [
            "pw-cli",
            "enum-params",
            &DEVICE_ID.to_string(),
            "EnumProfile"
        ]
    );
    assert_eq!(
        calls[2],
        ["pactl", "set-card-profile", DEVICE_NAME, "a2dp-source"]
    );
    assert_eq!(calls[3], ["pw-dump"]);
    assert_eq!(calls[4], ["pactl", "get-default-sink"]);
    assert_eq!(
        log.detached(),
        vec![(
            vec![
                "pw-loopback".into(),
                "-C".into(),
                SOURCE_NODE.into(),
                "-P".into(),
                SINK.into()
            ],
            1000
        )]
    );
    // state.json got the pid
    let stored: StateFile =
        serde_json::from_str(&std::fs::read_to_string(&state).unwrap()).unwrap();
    assert_eq!(stored.loopback_pid, Some(1000));
}

#[test]
fn turn_on_pid_write_preserves_last_seen() {
    // S4: turn_on writes the loopback pid via a load-modify-save; the
    // remembered phone must survive that write (a pid write is never an
    // excuse to drop last_seen, and vice versa).
    let dir = tmp_dir("turn-on-preserve");
    let state = dir.join("state.json");
    let stale = std::process::id(); // not a pw-loopback -> not alive -> respawn
    std::fs::write(
        &state,
        format!(
            "{{\"loopback_pid\": {stale}, \"last_seen\": {{\"mac\": \"{MAC}\", \"name\": \"Maxii\", \"device_name\": \"{DEVICE_NAME}\"}}}}"
        ),
    )
    .unwrap();
    let responses = vec![
        one_phone_dump("a2dp-source"), // discovery
        FakeRunner::ok(enum_profiles_text()),
        one_phone_dump("a2dp-source"), // wait_for_source finds the node
        default_sink_out(),
    ];
    let fake = FakeRunner::new(responses);
    let mut app = App::with_paths(Box::new(fake), dir.join("config.json"), state.clone(), None);
    app.turn_on().unwrap();

    let stored: StateFile =
        serde_json::from_str(&std::fs::read_to_string(&state).unwrap()).unwrap();
    assert_eq!(stored.loopback_pid, Some(1000), "pid written by turn_on");
    let last = stored.last_seen.expect("pid write must keep last_seen");
    assert_eq!(last.mac, MAC);
    assert_eq!(last.name, "Maxii");
}

#[test]
fn turn_on_picks_audio_gateway_on_classless_bluez_card() {
    // Real bluez cards list only off + audio-gateway and expose no classes
    // struct; the app must still pick a receive profile by name.
    let dir = tmp_dir("turn-on-gateway");
    let cfg = dir.join("config.json");
    let state = dir.join("state.json");
    let responses = vec![
        one_phone_dump("off"),                 // phone discovery
        FakeRunner::ok(enum_classless_text()), // off + audio-gateway, classless
        FakeRunner::ok(""),                    // pactl set-card-profile audio-gateway
        one_phone_dump("audio-gateway"),       // poll finds bluez_input node
        default_sink_out(),
    ];
    let fake = FakeRunner::new(responses);
    let log = fake.log();
    let mut app = App::with_paths(Box::new(fake), cfg.clone(), state.clone(), None);
    app.turn_on().unwrap();

    let calls = log.calls();
    assert_eq!(
        calls[2],
        ["pactl", "set-card-profile", DEVICE_NAME, "audio-gateway"]
    );
    assert_eq!(
        log.detached(),
        vec![(
            vec![
                "pw-loopback".into(),
                "-C".into(),
                SOURCE_NODE.into(),
                "-P".into(),
                SINK.into()
            ],
            1000
        )]
    );
}

#[test]
fn turn_on_restarts_when_stored_pid_is_not_a_live_loopback() {
    let dir = tmp_dir("turn-on-noop");
    let state = dir.join("state.json");
    let _seed_pid = seed_state_pid(&state); // stale pid: not our loopback -> not alive
    let responses = vec![
        one_phone_dump("a2dp-source"), // discovery
        FakeRunner::ok(enum_profiles_text()),
        one_phone_dump("a2dp-source"), // poll
        default_sink_out(),
    ];
    let fake = FakeRunner::new(responses);
    let log = fake.log();
    let mut app = App::with_paths(Box::new(fake), dir.join("config.json"), state.clone(), None);
    app.turn_on().unwrap();

    let calls = log.calls();
    // Profile already a2dp-source -> no set-card-profile; stale pid -> loopback respawned.
    assert!(!calls
        .iter()
        .any(|c| c[0] == "pactl" && c[1] == "set-card-profile"));
    assert_eq!(calls[3], ["pactl", "get-default-sink"]);
    assert_eq!(
        log.detached(),
        vec![(
            vec![
                "pw-loopback".into(),
                "-C".into(),
                SOURCE_NODE.into(),
                "-P".into(),
                SINK.into()
            ],
            1000
        )]
    );
    let stored: StateFile =
        serde_json::from_str(&std::fs::read_to_string(&state).unwrap()).unwrap();
    assert_eq!(stored.loopback_pid, Some(1000));
}

#[test]
fn turn_on_errors_without_phone() {
    let mut app = App::with_runner(
        Box::new(FakeRunner::new(vec![dump_json(&[], &[alsa_sink()])])),
        None,
    );
    let err = app.turn_on().unwrap_err();
    assert!(err.to_string().contains("no bluetooth phone"));
}

#[test]
fn turn_on_without_phone_or_config_makes_no_reconnect_attempt() {
    // No device and no configured MAC: actionable error, and no
    // disconnect/connect commands may be issued.
    let fake = FakeRunner::new(vec![dump_json(&[], &[alsa_sink()])]);
    let log = fake.log();
    let mut app = App::with_runner(Box::new(fake), None);
    let err = app.turn_on().unwrap_err();
    assert!(err.to_string().contains("no bluetooth phone"));
    assert!(
        !log.calls().iter().any(|c| c[0] == "bluetoothctl"),
        "no reconnect attempt without a configured phone"
    );
}

#[test]
fn turn_on_reconnects_configured_phone_when_absent() {
    // Discovery finds nothing for the configured MAC -> bluetoothctl connect,
    // then the card reappears -> normal profile flow proceeds; the reconnect
    // happens before any profile work, and the loopback still routes.
    let responses = vec![
        dump_json(&[], &[alsa_sink()]),       // discovery: phone absent
        FakeRunner::ok(""),                   // bluetoothctl connect
        one_phone_dump("off"),                // wait_for_card poll finds the card
        one_phone_dump("off"),                // re-scan after reconnect
        FakeRunner::ok(enum_profiles_text()), // enum-params
        FakeRunner::ok(""),                   // pactl set-card-profile a2dp-source
        one_phone_dump("a2dp-source"),        // wait_for_source finds the node
        default_sink_out(),
    ];
    let fake = FakeRunner::new(responses);
    let log = fake.log();
    let dir = tmp_dir("turn-on-reconnect");
    let mut app = App::with_paths(
        Box::new(fake),
        dir.join("config.json"),
        dir.join("state.json"),
        Some(MAC.into()),
    );
    app.turn_on().unwrap();
    let calls = log.calls();
    assert_eq!(calls[0], ["pw-dump"]);
    assert_eq!(calls[1], ["bluetoothctl", "connect", MAC]);
    assert_eq!(calls[2], ["pw-dump"], "wait_for_card polls for the card");
    assert_eq!(calls[3], ["pw-dump"], "re-scan after reconnect");
    assert_eq!(
        calls[5],
        ["pactl", "set-card-profile", DEVICE_NAME, "a2dp-source"]
    );
    assert_eq!(
        log.detached(),
        vec![(
            vec![
                "pw-loopback".into(),
                "-C".into(),
                SOURCE_NODE.into(),
                "-P".into(),
                SINK.into()
            ],
            1000
        )]
    );
}

#[test]
fn turn_on_reconnects_last_seen_phone_without_config() {
    // After `off` the phone is gone from the scan; with no configured MAC but
    // a last_seen phone in state, turn_on must reconnect that phone and then
    // proceed with the normal profile flow once its card reappears.
    let dir = tmp_dir("turn-on-last-seen");
    let state = dir.join("state.json");
    std::fs::write(
        &state,
        format!(
            "{{\"last_seen\": {{\"mac\": \"{MAC}\", \"name\": \"Maxii\", \"device_name\": \"{DEVICE_NAME}\"}}}}"
        ),
    )
    .unwrap();
    let responses = vec![
        dump_json(&[], &[alsa_sink()]),       // discovery: phone absent
        FakeRunner::ok(""),                   // bluetoothctl connect (best-effort)
        one_phone_dump("off"),                // wait_for_card poll finds the card
        one_phone_dump("off"),                // re-scan after reconnect
        FakeRunner::ok(enum_profiles_text()), // enum-params
        FakeRunner::ok(""),                   // pactl set-card-profile a2dp-source
        one_phone_dump("a2dp-source"),        // wait_for_source finds the node
        default_sink_out(),
    ];
    let fake = FakeRunner::new(responses);
    let log = fake.log();
    let mut app = App::with_paths(Box::new(fake), dir.join("config.json"), state.clone(), None);
    app.turn_on().unwrap();
    let calls = log.calls();
    assert_eq!(calls[0], ["pw-dump"]);
    assert_eq!(calls[1], ["bluetoothctl", "connect", MAC]);
    assert_eq!(calls[2], ["pw-dump"], "wait_for_card polls for the card");
    assert_eq!(calls[3], ["pw-dump"], "re-scan after reconnect");
    assert_eq!(
        calls[5],
        ["pactl", "set-card-profile", DEVICE_NAME, "a2dp-source"]
    );
    assert_eq!(
        log.detached(),
        vec![(
            vec![
                "pw-loopback".into(),
                "-C".into(),
                SOURCE_NODE.into(),
                "-P".into(),
                SINK.into()
            ],
            1000
        )]
    );
}

#[test]
fn turn_on_polls_until_source_node_appears_without_consuming_full_budget() {
    // The bluez_input node appears only on the third wait-for-source scan; the
    // poll loop must find it (with the bounded 500 ms interval, not a 30 s
    // sleep), then spawn the loopback.
    let responses = vec![
        one_phone_dump("off"), // discovery
        FakeRunner::ok(enum_profiles_text()),
        FakeRunner::ok(""), // pactl set-card-profile a2dp-source
        dump_json(&[bluez_device("a2dp-source")], &[alsa_sink()]), // poll 1: no node
        dump_json(&[bluez_device("a2dp-source")], &[alsa_sink()]), // poll 2: no node
        one_phone_dump("a2dp-source"), // poll 3: node appears
        default_sink_out(),
    ];
    let fake = FakeRunner::new(responses);
    let log = fake.log();
    let dir = tmp_dir("turn-on-poll");
    let mut app = App::with_paths(
        Box::new(fake),
        dir.join("config.json"),
        dir.join("state.json"),
        None,
    );
    let start = std::time::Instant::now();
    app.turn_on().unwrap();
    // Bound: the ~30 s budget must not be consumed — a few 500 ms polls at most.
    assert!(
        start.elapsed() < std::time::Duration::from_secs(10),
        "wait must be bounded, took {:?}",
        start.elapsed()
    );
    let calls = log.calls();
    let dumps = calls.iter().filter(|c| c[0] == "pw-dump").count();
    assert_eq!(dumps, 4, "discovery + 3 polls, then the loopback spawns");
    assert_eq!(calls[3], ["pw-dump"]);
    assert_eq!(calls[4], ["pw-dump"]);
    assert_eq!(calls[5], ["pw-dump"]);
    assert_eq!(calls[6], ["pactl", "get-default-sink"]);
    assert_eq!(log.detached().len(), 1);
}

#[test]
fn turn_on_kills_stale_alive_loopback_and_rearms_on_current_node() {
    // The reported bug: the phone's stream restarted, the bluez_input node
    // incremented (.1 -> .2), and the old pw-loopback keeps running against the
    // dead name while state records it. turn_on must see "alive pid but stale
    // node", kill the old loopback (exactly like turn_off), and spawn a fresh
    // one against the current node, saving pid + node.
    let dir = tmp_dir("turn-on-rearm");
    let state = dir.join("state.json");
    let mut child = seed_live_state(&state, Some(SOURCE_NODE)); // recorded node: .1
    let responses = vec![
        dump_json(
            &[bluez_device("a2dp-source")],
            &[source_node_named(SOURCE_NODE_2), alsa_sink()],
        ), // discovery: current node .2
        FakeRunner::ok(enum_profiles_text()),
        dump_json(
            &[bluez_device("a2dp-source")],
            &[source_node_named(SOURCE_NODE_2), alsa_sink()],
        ), // wait_for_source finds .2
        FakeRunner::ok(""), // kill old pid
        default_sink_out(),
    ];
    let fake = FakeRunner::new(responses);
    let log = fake.log();
    let mut app = App::with_paths(Box::new(fake), dir.join("config.json"), state.clone(), None);
    app.turn_on().unwrap();

    let calls = log.calls();
    assert!(
        calls
            .iter()
            .any(|c| c[0] == "kill" && c[1] == child.id().to_string()),
        "stale live loopback must be killed"
    );
    assert_eq!(
        log.detached(),
        vec![(
            vec![
                "pw-loopback".into(),
                "-C".into(),
                SOURCE_NODE_2.into(),
                "-P".into(),
                SINK.into()
            ],
            1000
        )],
        "fresh loopback captured against the CURRENT node"
    );
    let stored: StateFile =
        serde_json::from_str(&std::fs::read_to_string(&state).unwrap()).unwrap();
    assert_eq!(stored.loopback_pid, Some(1000), "new pid persisted");
    assert_eq!(
        stored.loopback_node.as_deref(),
        Some(SOURCE_NODE_2),
        "current node persisted"
    );

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn turn_on_with_matching_alive_pid_and_node_is_a_noop() {
    // Repeated `on` while the stream is stable: alive pid AND stored node ==
    // current node -> already on, no kill, no respawn, no state rewrite.
    let dir = tmp_dir("turn-on-noop");
    let state = dir.join("state.json");
    let mut child = seed_live_state(&state, Some(SOURCE_NODE));
    let fake = FakeRunner::new(vec![
        one_phone_dump("a2dp-source"), // discovery
        FakeRunner::ok(enum_profiles_text()),
        one_phone_dump("a2dp-source"), // wait_for_source: same node
    ]);
    let log = fake.log();
    let mut app = App::with_paths(Box::new(fake), dir.join("config.json"), state.clone(), None);
    app.turn_on().unwrap();

    assert!(
        !log.calls().iter().any(|c| c[0] == "kill"),
        "no kill when already on"
    );
    assert!(log.detached().is_empty(), "no respawn when already on");
    let stored: StateFile =
        serde_json::from_str(&std::fs::read_to_string(&state).unwrap()).unwrap();
    assert_eq!(stored.loopback_pid, Some(child.id()), "pid untouched");
    assert_eq!(stored.loopback_node.as_deref(), Some(SOURCE_NODE));

    let _ = child.kill();
    let _ = child.wait();
}

// ---- turn_off ------------------------------------------------------------

#[test]
fn turn_off_with_stale_pid_clears_state_and_drops_profile_without_killing() {
    let dir = tmp_dir("turn-off");
    let state = dir.join("state.json");
    let _seed_pid = seed_state_pid(&state); // stale pid: kill must be skipped
    let responses = vec![
        one_phone_dump("a2dp-source"), // scan for phone
        FakeRunner::ok(""),            // pactl set-card-profile off
        FakeRunner::ok(""),            // bluetoothctl disconnect
    ];
    let fake = FakeRunner::new(responses);
    let log = fake.log();
    let mut app = App::with_paths(Box::new(fake), dir.join("config.json"), state.clone(), None);
    app.turn_off().unwrap();

    let calls = log.calls();
    assert!(
        !calls.iter().any(|c| c[0] == "kill"),
        "no kill for a dead pid"
    );
    assert_eq!(calls[0], ["pw-dump"]);
    assert_eq!(calls[1], ["pactl", "set-card-profile", DEVICE_NAME, "off"]);
    assert_eq!(calls[2], ["bluetoothctl", "disconnect", MAC]);
    assert!(!state.exists(), "state.json removed");
}

#[test]
fn turn_off_kills_live_loopback_drops_profile_and_disconnects() {
    let dir = tmp_dir("turn-off-live");
    let state = dir.join("state.json");
    // A real process named pw-loopback so pid_alive() reports it live.
    let mut child = spawn_loopback(&dir.join("pw-loopback"));
    std::fs::write(&state, format!("{{\"loopback_pid\": {}}}", child.id())).unwrap();

    // Responses follow the run() order: kill, scan, pactl off, disconnect.
    let responses = vec![
        FakeRunner::ok(""),            // kill (result unused by turn_off)
        one_phone_dump("a2dp-source"), // scan
        FakeRunner::ok(""),            // pactl set-card-profile off
        FakeRunner::ok(""),            // bluetoothctl disconnect
    ];
    let fake = FakeRunner::new(responses);
    let log = fake.log();
    let mut app = App::with_paths(Box::new(fake), dir.join("config.json"), state.clone(), None);
    app.turn_off().unwrap();

    let calls = log.calls();
    assert_eq!(calls[0], ["kill", &child.id().to_string()]);
    assert_eq!(calls[1], ["pw-dump"]);
    assert_eq!(calls[2], ["pactl", "set-card-profile", DEVICE_NAME, "off"]);
    assert_eq!(calls[3], ["bluetoothctl", "disconnect", MAC]);
    assert!(!state.exists(), "state.json removed");

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn turn_off_disconnects_configured_phone_when_card_already_gone() {
    // The card vanished (e.g. a disconnect raced it away): the profile drop is
    // skipped and the disconnect still goes out for the configured MAC, on a
    // best-effort basis (a failed disconnect is tolerated, not an error).
    let responses = vec![
        dump_json(&[], &[alsa_sink()]), // scan: phone already gone
        CmdOut {
            status: 1,
            stdout: String::new(),
            stderr: "Device 28:8F:F6:71:6F:6E not available\n".into(),
        },
    ];
    let fake = FakeRunner::new(responses);
    let log = fake.log();
    let mut app = App::with_runner(Box::new(fake), Some(MAC.into()));
    app.turn_off().unwrap(); // tolerant: failure to disconnect is not fatal

    let calls = log.calls();
    assert_eq!(calls[0], ["pw-dump"]);
    assert_eq!(calls[1], ["bluetoothctl", "disconnect", MAC]);
    assert!(
        !calls.iter().any(|c| c[0] == "pactl"),
        "no profile drop for a missing card"
    );
}

#[test]
fn turn_off_with_vanished_pid_still_runs_profile_off_and_disconnect() {
    // The loopback died before `off` ran (the race the /proc re-check guards):
    // the pid is gone, so no kill is issued and the rest still completes.
    let dir = tmp_dir("turn-off-vanished");
    let state = dir.join("state.json");
    let mut child = spawn_loopback(&dir.join("pw-loopback"));
    let pid = child.id();
    let _ = child.kill();
    let _ = child.wait(); // reaped: /proc/<pid> is gone before turn_off runs

    std::fs::write(&state, format!("{{\"loopback_pid\": {pid}}}")).unwrap();
    let responses = vec![
        one_phone_dump("a2dp-source"), // scan
        FakeRunner::ok(""),            // pactl set-card-profile off
        FakeRunner::ok(""),            // bluetoothctl disconnect
    ];
    let fake = FakeRunner::new(responses);
    let log = fake.log();
    let mut app = App::with_paths(Box::new(fake), dir.join("config.json"), state.clone(), None);
    app.turn_off().unwrap();

    let calls = log.calls();
    assert!(
        !calls.iter().any(|c| c[0] == "kill"),
        "no kill for a vanished pid"
    );
    assert_eq!(calls[0], ["pw-dump"]);
    assert_eq!(calls[1], ["pactl", "set-card-profile", DEVICE_NAME, "off"]);
    assert_eq!(calls[2], ["bluetoothctl", "disconnect", MAC]);
}

#[test]
fn turn_off_kill_failure_still_runs_profile_off_and_disconnect() {
    // The loopback is alive right up to the kill, but the kill command fails
    // (e.g. EPERM): off must warn and still drop the profile + disconnect.
    let dir = tmp_dir("turn-off-kill-fail");
    let state = dir.join("state.json");
    let mut child = spawn_loopback(&dir.join("pw-loopback"));
    std::fs::write(&state, format!("{{\"loopback_pid\": {}}}", child.id())).unwrap();

    let responses = vec![
        CmdOut {
            status: 1,
            stdout: String::new(),
            stderr: "Operation not permitted\n".into(),
        },
        one_phone_dump("a2dp-source"), // scan
        FakeRunner::ok(""),            // pactl set-card-profile off
        FakeRunner::ok(""),            // bluetoothctl disconnect
    ];
    let fake = FakeRunner::new(responses);
    let log = fake.log();
    let mut app = App::with_paths(Box::new(fake), dir.join("config.json"), state.clone(), None);
    app.turn_off().unwrap();

    let calls = log.calls();
    assert_eq!(
        calls[0],
        ["kill", &child.id().to_string()],
        "kill was attempted"
    );
    assert_eq!(calls[1], ["pw-dump"]);
    assert_eq!(calls[2], ["pactl", "set-card-profile", DEVICE_NAME, "off"]);
    assert_eq!(calls[3], ["bluetoothctl", "disconnect", MAC]);

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn turn_off_keeps_last_seen_phone_so_status_still_finds_it() {
    // The core of T-LIVE-5: after `off` the pid is cleared but the remembered
    // phone survives, so a fresh status still reports it (available, reconnect
    // reason) and the toggle can bring it back.
    let dir = tmp_dir("turn-off-keep");
    let state = dir.join("state.json");
    let mut child = spawn_loopback(&dir.join("pw-loopback"));
    std::fs::write(
        &state,
        format!(
            "{{\"loopback_pid\": {}, \"last_seen\": {{\"mac\": \"{MAC}\", \"name\": \"Maxii\", \"device_name\": \"{DEVICE_NAME}\"}}}}",
            child.id()
        ),
    )
    .unwrap();

    let responses = vec![
        FakeRunner::ok(""),            // kill
        one_phone_dump("a2dp-source"), // scan
        FakeRunner::ok(""),            // pactl set-card-profile off
        FakeRunner::ok(""),            // bluetoothctl disconnect
    ];
    let fake = FakeRunner::new(responses);
    let log = fake.log();
    let mut app = App::with_paths(Box::new(fake), dir.join("config.json"), state.clone(), None);
    app.turn_off().unwrap();

    let calls = log.calls();
    assert_eq!(calls[0], ["kill", &child.id().to_string()]);
    assert_eq!(calls[2], ["pactl", "set-card-profile", DEVICE_NAME, "off"]);
    assert_eq!(calls[3], ["bluetoothctl", "disconnect", MAC]);
    let stored: StateFile =
        serde_json::from_str(&std::fs::read_to_string(&state).unwrap()).unwrap();
    assert!(stored.loopback_pid.is_none(), "pid cleared on off");
    assert_eq!(
        stored.last_seen.as_ref().unwrap().mac,
        MAC,
        "last_seen survives off"
    );

    // The post-`off` status: empty scan, remembered phone -> available + reason.
    let mut app2 = App::with_paths(
        Box::new(FakeRunner::new(vec![dump_json(&[], &[alsa_sink()])])),
        dir.join("config.json"),
        state,
        None,
    );
    let s = app2.status().unwrap();
    assert!(s.available);
    assert!(!s.on);
    assert_eq!(s.phones.len(), 1);
    assert_eq!(s.phones[0].mac, MAC);
    assert_eq!(s.reason, RECONNECT_REASON);

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn turn_off_clears_pid_and_node_from_state() {
    // The state schema change: off must clear the recorded capture node along
    // with the pid (clear_state covers the new field), while keeping the
    // remembered phone.
    let dir = tmp_dir("turn-off-node");
    let state = dir.join("state.json");
    let mut child = spawn_loopback(&dir.join("pw-loopback"));
    std::fs::write(
        &state,
        format!(
            "{{\"loopback_pid\": {}, \"loopback_node\": \"{SOURCE_NODE}\", \"last_seen\": {{\"mac\": \"{MAC}\", \"name\": \"Maxii\", \"device_name\": \"{DEVICE_NAME}\"}}}}",
            child.id()
        ),
    )
    .unwrap();
    let responses = vec![
        FakeRunner::ok(""),            // kill
        one_phone_dump("a2dp-source"), // scan
        FakeRunner::ok(""),            // pactl set-card-profile off
        FakeRunner::ok(""),            // bluetoothctl disconnect
    ];
    let fake = FakeRunner::new(responses);
    let mut app = App::with_paths(Box::new(fake), dir.join("config.json"), state.clone(), None);
    app.turn_off().unwrap();

    let stored: StateFile =
        serde_json::from_str(&std::fs::read_to_string(&state).unwrap()).unwrap();
    assert!(stored.loopback_pid.is_none(), "pid cleared on off");
    assert!(stored.loopback_node.is_none(), "node cleared on off");
    assert_eq!(
        stored.last_seen.as_ref().unwrap().mac,
        MAC,
        "last_seen survives off"
    );

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn turn_off_state_write_failure_still_drops_profile_and_disconnects() {
    // W2: a read-only state dir makes clear_state() fail (EACCES writing the
    // tmp file). off must warn and STILL kill the loopback, drop the profile
    // and disconnect — no state write error may abort the request. (Note: as
    // root the chmod does not block writes, so the assertions here drive on
    // the commands that must run either way.)
    let dir = tmp_dir("turn-off-state-fail");
    let state = dir.join("state.json");
    let mut child = spawn_loopback(&dir.join("pw-loopback"));
    std::fs::write(&state, format!("{{\"loopback_pid\": {}}}", child.id())).unwrap();
    let mut perms = std::fs::metadata(&dir).unwrap().permissions();
    perms.set_mode(0o500); // r-x: state.json readable, tmp write/rename denied
    std::fs::set_permissions(&dir, perms).unwrap();

    let responses = vec![
        FakeRunner::ok(""),            // kill
        one_phone_dump("a2dp-source"), // scan
        FakeRunner::ok(""),            // pactl set-card-profile off
        FakeRunner::ok(""),            // bluetoothctl disconnect
    ];
    let fake = FakeRunner::new(responses);
    let log = fake.log();
    let mut app = App::with_paths(Box::new(fake), dir.join("config.json"), state, None);
    app.turn_off().unwrap();

    let calls = log.calls();
    assert_eq!(calls[0], ["kill", &child.id().to_string()]);
    assert_eq!(calls[2], ["pactl", "set-card-profile", DEVICE_NAME, "off"]);
    assert_eq!(calls[3], ["bluetoothctl", "disconnect", MAC]);

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn forget_drops_last_seen_keeps_pid_and_status_reports_no_phone() {
    // W3: after unpairing, a stale last_seen keeps reporting an available
    // phone forever. `forget` drops only last_seen — the loopback pid is
    // untouched and the file stays valid; status then reports no phone.
    let dir = tmp_dir("forget");
    let state = dir.join("state.json");
    let mut child = spawn_loopback(&dir.join("pw-loopback"));
    std::fs::write(
        &state,
        format!(
            "{{\"loopback_pid\": {}, \"last_seen\": {{\"mac\": \"{MAC}\", \"name\": \"Maxii\", \"device_name\": \"{DEVICE_NAME}\"}}}}",
            child.id()
        ),
    )
    .unwrap();
    let mut app = App::with_paths(
        Box::new(FakeRunner::new(vec![])),
        dir.join("config.json"),
        state.clone(),
        None,
    );
    app.forget().unwrap();

    let stored: StateFile =
        serde_json::from_str(&std::fs::read_to_string(&state).unwrap()).unwrap();
    assert_eq!(stored.last_seen, None, "last_seen dropped by forget");
    assert_eq!(
        stored.loopback_pid,
        Some(child.id()),
        "forget keeps the loopback pid"
    );

    // With nothing remembered, an empty scan reports no phone again.
    let mut app2 = App::with_paths(
        Box::new(FakeRunner::new(vec![dump_json(&[], &[alsa_sink()])])),
        dir.join("config.json"),
        state,
        None,
    );
    let s = app2.status().unwrap();
    assert!(!s.available, "forgotten phone is no longer available");
    assert!(s.phone.is_none());

    let _ = child.kill();
    let _ = child.wait();
}

// ---- volume / set-phone / config -----------------------------------------

#[test]
fn set_volume_targets_bluez_source_node() {
    let fake = FakeRunner::new(vec![one_phone_dump("a2dp-source"), FakeRunner::ok("")]);
    let log = fake.log();
    let mut app = App::with_runner(Box::new(fake), None);
    app.set_volume(65.0).unwrap();
    let calls = log.calls();
    assert_eq!(calls[1], ["wpctl", "set-volume", "51", "0.65"]);
}

#[test]
fn set_volume_errors_without_source_node() {
    let mut app = App::with_runner(
        Box::new(FakeRunner::new(vec![dump_json(
            &[bluez_device("off")],
            &[alsa_sink()],
        )])),
        None,
    );
    let err = app.set_volume(50.0).unwrap_err();
    assert!(err.to_string().contains("source node"));
}

#[test]
fn set_phone_matches_by_mac_or_name_and_config_roundtrips() {
    let dir = tmp_dir("config");
    let cfg = dir.join("config.json");
    let state = dir.join("state.json");
    let mut app = App::with_paths(
        Box::new(FakeRunner::new(vec![one_phone_dump("a2dp-source")])),
        cfg.clone(),
        state.clone(),
        None,
    );

    let picked = app.set_phone("maxii").unwrap(); // case-insensitive name
    assert_eq!(picked.mac, MAC);

    // config.json written for real
    let cfg_val: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&cfg).unwrap()).unwrap();
    assert_eq!(cfg_val["phone_mac"], MAC);

    // a fresh app on the same dirs picks the mac up from disk
    let mut app2 = App::with_paths(
        Box::new(FakeRunner::new(vec![
            one_phone_dump("a2dp-source"),
            FakeRunner::ok("Volume: 1.00\n"),
        ])),
        cfg.clone(),
        state,
        None,
    );
    let s = app2.status().unwrap();
    assert_eq!(s.phone.as_ref().unwrap().mac, MAC);

    // unknown phone -> error
    let err = App::with_runner(
        Box::new(FakeRunner::new(vec![one_phone_dump("a2dp-source")])),
        None,
    )
    .set_phone("nope")
    .unwrap_err();
    assert!(err.to_string().contains("no bluetooth phone matches"));
}
