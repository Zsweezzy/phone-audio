//! Behavior tests for [`super::App`] against a scripted [`FakeRunner`].
//! These never touch the real machine: no phone, no PipeWire required.

use std::collections::VecDeque;
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
}

const MAC: &str = "28:8F:F6:71:6F:6E";
const DEVICE_NAME: &str = "bluez_card.28_8F_F6_71_6F_6E";
const DEVICE_ID: u32 = 45;
const SOURCE_NODE: &str = "bluez_input.28_8F_F6_71_6F_6E.1";
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
    // Real pipewire 1.6.9 emits the bluez input node as Stream/Output/Audio,
    // not Audio/Source — the app must find it by node.name, not media.class.
    serde_json::json!({
        "id": 51,
        "type": "PipeWire:Interface:Node",
        "info": { "props": {
            "media.class": "Stream/Output/Audio",
            "device.api": "bluez5",
            "node.name": SOURCE_NODE,
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

fn tmp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("pa-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn seed_state_pid(state: &Path) -> u32 {
    // A pid that is *not* a live pw-loopback (identity check in pid_alive):
    // this test process's /proc entry exists but its comm is not "pw-loopback",
    // exactly the stale/zombie-pid situation fix 1 protects against.
    let pid = std::process::id();
    std::fs::write(state, format!("{{\"loopback_pid\": {pid}}}")).unwrap();
    pid
}

fn default_sink_out() -> CmdOut {
    FakeRunner::ok(&format!("{SINK}\n"))
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
fn status_with_no_phone_is_unavailable() {
    let mut app = App::with_runner(
        Box::new(FakeRunner::new(vec![dump_json(&[], &[alsa_sink()])])),
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
    let mut app = App::with_runner(
        Box::new(FakeRunner::new(vec![dump_json(
            &[bluez_device("off"), other_device()],
            &[],
        )])),
        None,
    );
    let s = app.status().unwrap();
    assert!(s.available);
    assert!(s.phone.is_none());
    assert!(s.reason.contains("set-phone"));
}

#[test]
fn status_with_single_phone_reports_profile_and_volume() {
    let mut app = App::with_runner(
        Box::new(FakeRunner::new(vec![
            one_phone_dump("a2dp-source"),
            FakeRunner::ok("Volume: 0.65\n"),
        ])),
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
    let mut app = App::with_runner(Box::new(fake), None);
    let s = app.status().unwrap();
    assert_eq!(s.profile.as_deref(), Some("audio-gateway"));
}

#[test]
fn status_streaming_without_loopback_is_off_with_on_reason() {
    // Real state (pipewire 1.6.9): bluez5.profile stays "off" while the phone
    // streams and the bluez_input node exists. `on` must reflect the running
    // loopback, not the profile field.
    let fake = FakeRunner::new(vec![one_phone_dump("off"), FakeRunner::ok("Volume: 0.5\n")]);
    let mut app = App::with_runner(Box::new(fake), None);
    let s = app.status().unwrap();
    assert!(s.available);
    assert!(!s.on);
    assert_eq!(s.profile.as_deref(), Some("off"));
    assert_eq!(s.reason, "streaming — run 'phone-audio on'");
    assert_eq!(s.volume, Some(50.0));
}

#[test]
fn status_profile_off_without_source_reports_profile_off_reason() {
    let mut app = App::with_runner(
        Box::new(FakeRunner::new(vec![dump_json(
            &[bluez_device("off")],
            &[alsa_sink()],
        )])),
        None,
    );
    let s = app.status().unwrap();
    assert!(!s.on);
    assert_eq!(s.reason, "profile off — run 'phone-audio on'");
}

#[test]
fn status_non_receive_profile_without_source_reports_not_streaming_reason() {
    let mut app = App::with_runner(
        Box::new(FakeRunner::new(vec![dump_json(
            &[bluez_device("a2dp-sink")],
            &[alsa_sink()],
        )])),
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
    let sleep = ["/bin/sleep", "/usr/bin/sleep"]
        .into_iter()
        .find(|p| std::path::Path::new(p).exists())
        .expect("no sleep binary to copy");
    std::fs::copy(sleep, &exe).unwrap();
    let mut child = std::process::Command::new(&exe).arg("30").spawn().unwrap();
    // exec runs just after spawn; wait until /proc reports the chosen name.
    for _ in 0..100 {
        let comm = std::fs::read_to_string(format!("/proc/{}/comm", child.id()));
        if comm.map(|s| s.trim() == "pw-loopback").unwrap_or(false) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    std::fs::write(&state, format!("{{\"loopback_pid\": {}}}", child.id())).unwrap();

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

// ---- turn_off ------------------------------------------------------------

#[test]
fn turn_off_with_stale_pid_clears_state_and_drops_profile_without_killing() {
    let dir = tmp_dir("turn-off");
    let state = dir.join("state.json");
    let _seed_pid = seed_state_pid(&state); // stale pid: kill must be skipped
    let responses = vec![
        one_phone_dump("a2dp-source"), // scan for phone
        FakeRunner::ok(""),            // pactl set-card-profile off
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
    assert!(!state.exists(), "state.json removed");
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
