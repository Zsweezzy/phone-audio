//! Parsing helpers for the output of `pw-cli enum-params`, `pw-dump`, `wpctl`.

use serde_json::{Map, Value};

/// One entry of a `pw-cli enum-params <id> EnumProfile` block.
#[derive(Debug, Clone, PartialEq)]
pub struct Profile {
    pub index: u32,
    pub name: String,
    /// `available: Id 2` means the profile is available (SPA_PARAM_AVAILABILITY_yes).
    pub available: bool,
    /// The block's `classes` struct contains `String "Audio/Source"`.
    pub has_source: bool,
}

/// Profile names that receive phone (remote) audio. Bluez cards emit
/// class-less `EnumProfile` blocks (PipeWire 1.6.x), so a profile is also
/// receive-capable when its name is in this list — not just when it exposes an
/// `Audio/Source` class. Single source of truth for profile-name matching.
pub const RECEIVE_PROFILES: [&str; 6] = [
    "a2dp-source",
    "a2dp-duplex",
    "audio-gateway",
    "headset-head-unit",
    "headset-audio-gateway",
    "handsfree",
];

/// Parse `pw-cli enum-params ... EnumProfile` text (the pod-dump format) into
/// profile blocks. Tolerant: only looks for the interesting lines.
///
/// A new profile starts at a line containing both `Object:` and `Param:Profile`;
/// any other `Object:` line (a foreign block) closes the current profile so its
/// props cannot leak into a neighboring one. Within a profile, `Prop: key
/// ...:index/name/available` lines are followed by `Int N` / `String "..."` /
/// `Id N` value lines. `Audio/Source` appearing in the block marks the profile
/// as source-capable.
pub fn parse_profiles(text: &str) -> Vec<Profile> {
    let mut out = Vec::new();
    let mut cur: Option<Profile> = None;
    let mut key: Option<&str> = None;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with("Object:") {
            if let Some(p) = cur.take() {
                out.push(p);
            }
            cur = if t.contains("Param:Profile") {
                Some(Profile {
                    index: 0,
                    name: String::new(),
                    available: false,
                    has_source: false,
                })
            } else {
                None
            };
            key = None;
            continue;
        }
        let Some(p) = cur.as_mut() else { continue };
        if let Some(rest) = t.strip_prefix("Prop: key") {
            // `Spa:Pod:Object:Param:Profile:index (1), flags ...` -> `index`
            key = rest
                .split('(')
                .next()
                .and_then(|s| s.split(':').next_back())
                .map(str::trim);
            continue;
        }
        if t.starts_with("String \"Audio/Source\"") {
            p.has_source = true;
            continue;
        }
        match key {
            Some("index") => {
                if let Some(v) = t.strip_prefix("Int ") {
                    p.index = v.trim().parse().unwrap_or(0);
                }
            }
            Some("name") => {
                if let Some(v) = t.strip_prefix("String ") {
                    p.name = v.trim().trim_matches('"').to_string();
                }
            }
            Some("available") => {
                // `Id 2 (Spa:Enum:ParamAvailability:yes)` -> available yes
                let mut it = t.split_whitespace();
                if it.next() == Some("Id") {
                    p.available = it.next().and_then(|s| s.parse::<u32>().ok()) == Some(2);
                }
            }
            _ => {}
        }
    }
    if let Some(p) = cur {
        out.push(p);
    }
    out
}

/// Pick the profile that receives phone (remote) audio:
/// available `a2dp-source`, else `a2dp-duplex`, else `audio-gateway`, else any
/// available source-capable profile (Audio/Source class **or** name in
/// [`RECEIVE_PROFILES`]), preferring `a2dp`-named ones over the first capable.
pub fn pick_receive_profile(profiles: &[Profile]) -> Option<&Profile> {
    let named = |name: &str| profiles.iter().find(|p| p.available && p.name == name);
    let capable: Vec<&Profile> = profiles
        .iter()
        .filter(|p| p.available && (p.has_source || RECEIVE_PROFILES.contains(&p.name.as_str())))
        .collect();
    named("a2dp-source")
        .or_else(|| named("a2dp-duplex"))
        .or_else(|| named("audio-gateway"))
        .or_else(|| capable.iter().find(|p| p.name.contains("a2dp")).copied())
        .or_else(|| capable.first().copied())
}

/// Extract the volume float from `wpctl get-volume` output (e.g. `Volume: 0.65`).
pub fn parse_volume(text: &str) -> Option<f64> {
    text.lines()
        .find_map(|l| l.trim().strip_prefix("Volume:")?.trim().parse::<f64>().ok())
        .map(|v| v.clamp(0.0, 1.0))
}

/// `info.props` of a pw-dump object.
pub fn props(v: &Value) -> Option<&Map<String, Value>> {
    v.get("info")?.get("props")?.as_object()
}

/// The pipewire object type string ("PipeWire:Interface:Device", ...).
pub fn obj_type(v: &Value) -> Option<&str> {
    v.get("type").and_then(|t| t.as_str())
}

/// The pipewire object id.
pub fn obj_id(v: &Value) -> u32 {
    v.get("id").and_then(|x| x.as_u64()).unwrap_or(0) as u32
}

/// `28:8F:F6:71:6F:6E` -> `28_8F_F6_71_6F_6E` (the form used in node/device names).
pub fn mac_underscored(mac: &str) -> String {
    mac.replace(':', "_")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_volume_line() {
        assert_eq!(parse_volume("Volume: 0.65"), Some(0.65));
        assert_eq!(parse_volume("Muted: yes\nVolume: 1.00\n"), Some(1.0));
        assert_eq!(parse_volume("Volume: 0"), Some(0.0));
        assert_eq!(parse_volume(""), None);
    }

    /// The real pod-dump text of `pw-cli enum-params <id> EnumProfile`
    /// (PipeWire 1.6.9, captured against ALSA device 52).
    const DEVICE_52: &str = r#"  Object: size 160, type Spa:Pod:Object:Param:Profile (262151), id Spa:Enum:ParamId:EnumProfile (8)
    Prop: key Spa:Pod:Object:Param:Profile:index (1), flags 00000000
      Int 0
    Prop: key Spa:Pod:Object:Param:Profile:name (2), flags 00000000
      String "off"
    Prop: key Spa:Pod:Object:Param:Profile:description (3), flags 00000000
      String "Off"
    Prop: key Spa:Pod:Object:Param:Profile:priority (4), flags 00000000
      Int 0
    Prop: key Spa:Pod:Object:Param:Profile:available (5), flags 00000000
      Id 2        (Spa:Enum:ParamAvailability:yes)
    Prop: key Spa:Pod:Object:Param:Profile:classes (7), flags 00000000
      Struct: size 224
        Int 2
        Struct: size 96
          String "Audio/Sink"
          Int 1
  Object: size 160, type Spa:Pod:Object:Param:Profile (262151), id Spa:Enum:ParamId:EnumProfile (8)
    Prop: key Spa:Pod:Object:Param:Profile:index (1), flags 00000000
      Int 1
    Prop: key Spa:Pod:Object:Param:Profile:name (2), flags 00000000
      String "output:analog-stereo+input:mono-fallback"
    Prop: key Spa:Pod:Object:Param:Profile:description (3), flags 00000000
      String "Analog Stereo Duplex"
    Prop: key Spa:Pod:Object:Param:Profile:priority (4), flags 00000000
      Int 6561
    Prop: key Spa:Pod:Object:Param:Profile:available (5), flags 00000000
      Id 0        (Spa:Enum:ParamAvailability:no)
    Prop: key Spa:Pod:Object:Param:Profile:classes (7), flags 00000000
      Struct: size 224
        Int 2
        Struct: size 96
          String "Audio/Sink"
          Int 1
        Struct: size 96
          String "Audio/Source"
          Int 1
"#;

    #[test]
    fn parses_real_device_52_enum_output() {
        let ps = parse_profiles(DEVICE_52);
        assert_eq!(ps.len(), 2);
        let off = &ps[0];
        assert_eq!(off.index, 0);
        assert_eq!(off.name, "off");
        assert!(off.available, "Id 2 = available");
        assert!(!off.has_source, "off has no Audio/Source class");
        let fallback = &ps[1];
        assert_eq!(fallback.index, 1);
        assert_eq!(fallback.name, "output:analog-stereo+input:mono-fallback");
        assert!(!fallback.available, "Id 0 = unavailable");
        assert!(fallback.has_source, "duplex exposes Audio/Source");
    }

    /// One `EnumProfile` object in the pod-dump format.
    fn obj(index: u32, name: &str, id: u32, classes: &[&str]) -> String {
        let yes = if id == 2 { "yes" } else { "no" };
        let mut s = format!(
            r#"  Object: size 160, type Spa:Pod:Object:Param:Profile (262151), id Spa:Enum:ParamId:EnumProfile (8)
    Prop: key Spa:Pod:Object:Param:Profile:index (1), flags 00000000
      Int {index}
    Prop: key Spa:Pod:Object:Param:Profile:name (2), flags 00000000
      String "{name}"
    Prop: key Spa:Pod:Object:Param:Profile:description (3), flags 00000000
      String "{name}"
    Prop: key Spa:Pod:Object:Param:Profile:priority (4), flags 00000000
      Int 0
    Prop: key Spa:Pod:Object:Param:Profile:available (5), flags 00000000
      Id {id}        (Spa:Enum:ParamAvailability:{yes})
    Prop: key Spa:Pod:Object:Param:Profile:classes (7), flags 00000000
      Struct: size 224
        Int {c}
"#,
            c = classes.len()
        );
        for c in classes {
            s.push_str(&format!(
                "        Struct: size 96\n          String \"{c}\"\n          Int 1\n"
            ));
        }
        s
    }

    /// A realistic bluez card: off, sink/source, duplex, headset, and one
    /// unavailable profile (Id 0).
    fn bluez_fixture() -> String {
        let mut s = String::new();
        s.push_str(&obj(0, "off", 1, &["Audio/Sink"]));
        s.push_str(&obj(1, "a2dp-sink", 2, &["Audio/Sink"]));
        s.push_str(&obj(2, "a2dp-source", 2, &["Audio/Source"]));
        s.push_str(&obj(3, "a2dp-duplex", 2, &["Audio/Sink", "Audio/Source"]));
        s.push_str(&obj(
            4,
            "headset-head-unit",
            2,
            &["Audio/Sink", "Audio/Source"],
        ));
        s.push_str(&obj(5, "handsfree", 0, &["Audio/Sink", "Audio/Source"]));
        s
    }

    /// A junk non-Profile `Object:` block (a param from a different enum, e.g.
    /// `EnumParam`) carrying props that must not leak into a profile.
    fn junk_obj() -> String {
        String::from(
            r#"  Object: size 96, type Spa:Pod:Object:Param:EnumParam (131082), id Spa:Enum:ParamId:EnumParam (2)
    Prop: key Spa:Pod:Object:Param:EnumParam:index (1), flags 00000000
      Int 7
    Prop: key Spa:Pod:Object:Param:EnumParam:name (2), flags 00000000
      String "bogus"
"#,
        )
    }

    #[test]
    fn junk_object_block_between_profiles_leaks_nothing() {
        let text = format!(
            "{}{}{}",
            obj(0, "off", 1, &["Audio/Sink"]),
            junk_obj(),
            obj(1, "a2dp-source", 2, &["Audio/Source"])
        );
        let ps = parse_profiles(&text);
        assert_eq!(ps.len(), 2, "junk block contributes no profile");
        assert_eq!(ps[0].index, 0);
        assert_eq!(ps[0].name, "off");
        assert!(!ps[0].has_source);
        assert_eq!(ps[1].index, 1);
        assert_eq!(ps[1].name, "a2dp-source");
        assert!(ps[1].has_source);
    }

    #[test]
    fn profiles_parse_fields_and_availability() {
        let ps = parse_profiles(&bluez_fixture());
        assert_eq!(ps.len(), 6);
        assert_eq!(ps[0].index, 0);
        assert_eq!(ps[0].name, "off");
        assert!(!ps[0].available);
        assert!(!ps[0].has_source);
        assert_eq!(ps[1].name, "a2dp-sink");
        assert!(ps[1].available);
        assert!(!ps[1].has_source);
        assert_eq!(ps[2].name, "a2dp-source");
        assert!(ps[2].available && ps[2].has_source);
        assert_eq!(ps[3].name, "a2dp-duplex");
        assert!(ps[3].available && ps[3].has_source);
        assert_eq!(ps[4].name, "headset-head-unit");
        assert!(ps[4].available && ps[4].has_source);
        assert_eq!(ps[5].name, "handsfree");
        assert!(!ps[5].available);
    }

    #[test]
    fn pick_prefers_available_a2dp_source() {
        let ps = parse_profiles(&bluez_fixture());
        assert_eq!(pick_receive_profile(&ps).unwrap().name, "a2dp-source");
    }

    #[test]
    fn pick_falls_back_to_duplex_then_other_source() {
        let duplex = make("a2dp-duplex", true, true);
        let headset = make("headset-head-unit", true, true);
        assert_eq!(
            pick_receive_profile(std::slice::from_ref(&duplex))
                .unwrap()
                .name,
            "a2dp-duplex"
        );
        // a2dp preferred over headset among generic Audio/Source profiles
        assert_eq!(
            pick_receive_profile(&[headset, duplex]).unwrap().name,
            "a2dp-duplex"
        );
        // unavailable a2dp-source is skipped (nothing left -> None)
        let unavail = make("a2dp-source", false, true);
        assert!(pick_receive_profile(&[unavail]).is_none());
        let only_headset = make("headset-head-unit", true, true);
        assert_eq!(
            pick_receive_profile(&[only_headset]).unwrap().name,
            "headset-head-unit"
        );
        // no source-capable profile at all
        assert!(pick_receive_profile(&[make("off", true, false)]).is_none());
    }

    /// A bluez card profile block with NO `classes` struct at all — the real
    /// shape of `pw-cli enum-params <bluez-id> EnumProfile` (PipeWire 1.6.9).
    fn obj_classless(index: u32, name: &str, description: &str, id: u32) -> String {
        let yes = if id == 2 { "yes" } else { "no" };
        format!(
            r#"  Object: size 160, type Spa:Pod:Object:Param:Profile (262151), id Spa:Enum:ParamId:EnumProfile (8)
    Prop: key Spa:Pod:Object:Param:Profile:index (1), flags 00000000
      Int {index}
    Prop: key Spa:Pod:Object:Param:Profile:name (2), flags 00000000
      String "{name}"
    Prop: key Spa:Pod:Object:Param:Profile:description (3), flags 00000000
      String "{description}"
    Prop: key Spa:Pod:Object:Param:Profile:available (5), flags 00000000
      Id {id}        (Spa:Enum:ParamAvailability:{yes})
"#
        )
    }

    #[test]
    fn classless_bluez_profiles_are_source_capable_by_name() {
        // The real bluez card: only `off` and `audio-gateway`, nothing in a
        // `classes` struct, so no `Audio/Source` class string is present.
        let text = obj_classless(0, "off", "Off", 2)
            + &obj_classless(
                1,
                "audio-gateway",
                "Audio Gateway (A2DP Source & HSP/HFP AG)",
                2,
            );
        let ps = parse_profiles(&text);
        assert_eq!(ps.len(), 2);
        for p in &ps {
            assert!(p.available, "both profiles available on a live card");
            assert!(!p.has_source, "classless: no Audio/Source class string");
        }
        assert_eq!(
            pick_receive_profile(&ps).unwrap().name,
            "audio-gateway",
            "name-based capability picks audio-gateway on a classless card"
        );
    }

    #[test]
    fn pick_treats_named_but_classless_headset_as_capable() {
        // A profile whose name marks it capable is picked even without an
        // Audio/Source class (classless bluez EnumProfile blocks).
        for name in ["handsfree", "headset-head-unit", "headset-audio-gateway"] {
            let p = make(name, true, false);
            assert_eq!(
                pick_receive_profile(&[p]).unwrap().name,
                name,
                "{name} is a receive profile by name"
            );
        }
        // An unknown classless profile is not receive-capable.
        assert!(pick_receive_profile(&[make("bogus", true, false)]).is_none());
        assert!(pick_receive_profile(&[make("off", true, false)]).is_none());
        // audio-gateway beats a classless headset when both are available.
        let ps = [
            make("handsfree", true, false),
            make("audio-gateway", true, false),
        ];
        assert_eq!(pick_receive_profile(&ps).unwrap().name, "audio-gateway");
    }

    #[test]
    fn pick_prefers_named_profiles_in_order() {
        // a2dp-source -> a2dp-duplex -> audio-gateway -> first capable.
        let ps = [
            make("audio-gateway", true, false),
            make("a2dp-duplex", true, false),
            make("a2dp-source", true, false),
        ];
        assert_eq!(pick_receive_profile(&ps).unwrap().name, "a2dp-source");
        let ps = [
            make("audio-gateway", true, false),
            make("a2dp-duplex", true, false),
        ];
        assert_eq!(pick_receive_profile(&ps).unwrap().name, "a2dp-duplex");
        // Unavailable named profile is skipped; falls through to first capable.
        let ps = [
            make("a2dp-source", false, true),
            make("handsfree", true, false),
        ];
        assert_eq!(pick_receive_profile(&ps).unwrap().name, "handsfree");
    }

    #[test]
    fn receive_profiles_list_covers_all_receive_names() {
        for name in [
            "a2dp-source",
            "a2dp-duplex",
            "audio-gateway",
            "headset-head-unit",
            "headset-audio-gateway",
            "handsfree",
        ] {
            assert!(RECEIVE_PROFILES.contains(&name), "{name} missing");
        }
    }

    fn make(name: &str, available: bool, has_source: bool) -> Profile {
        Profile {
            index: 0,
            name: name.into(),
            available,
            has_source,
        }
    }
}
