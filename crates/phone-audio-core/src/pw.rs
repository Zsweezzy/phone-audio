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

/// Parse `pw-cli enum-params ... EnumProfile` text into profile blocks.
pub fn parse_profiles(text: &str) -> Vec<Profile> {
    let mut out = Vec::new();
    let mut cur: Option<Profile> = None;
    for line in text.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("index:") {
            if let Some(p) = cur.take() {
                out.push(p);
            }
            cur = Some(Profile {
                index: rest.trim().parse().unwrap_or(0),
                name: String::new(),
                available: false,
                has_source: false,
            });
        } else if let Some(p) = cur.as_mut() {
            if let Some(rest) = t.strip_prefix("name:") {
                p.name = rest.trim().trim_matches('"').to_string();
            } else if let Some(rest) = t.strip_prefix("available:") {
                p.available = rest
                    .split_whitespace()
                    .last()
                    .and_then(|s| s.parse::<u32>().ok())
                    == Some(2);
            } else if t.contains("Audio/Source") {
                p.has_source = true;
            }
        }
    }
    if let Some(p) = cur {
        out.push(p);
    }
    out
}

/// Pick the profile that receives phone (remote) audio:
/// available `a2dp-source`, else `a2dp-duplex`, else any available profile exposing an
/// Audio/Source class, preferring `a2dp`-named ones over `headset`.
pub fn pick_receive_profile(profiles: &[Profile]) -> Option<&Profile> {
    let named = |name: &str| profiles.iter().find(|p| p.available && p.name == name);
    named("a2dp-source")
        .or_else(|| named("a2dp-duplex"))
        .or_else(|| {
            let with_source: Vec<&Profile> = profiles
                .iter()
                .filter(|p| p.available && p.has_source)
                .collect();
            with_source
                .iter()
                .find(|p| p.name.contains("a2dp"))
                .copied()
                .or_else(|| with_source.first().copied())
        })
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

    #[test]
    fn profiles_parse_fields_and_availability() {
        let text = r#"id 0
	type PipeWire:Interface:Device
	cookie 32692
	bound-id 17
	object.serial 34566
	object.path "bluez:/org/bluez/hci0/dev_28_8F_F6_71_6F_6E"
	param EnumProfile:
		index:		0
		name:		"off"
		description:	"Off"
		priority:	0
		available:	Id 1
		classes:
			String "Audio/Sink"
			String "Audio/Source"
		index:		1
		name:		"a2dp-sink"
		description:	"High Fidelity Playback (A2DP Sink)"
		priority:	19000
		available:	Id 2
		classes:
			String "Audio/Sink"
		index:		2
		name:		"a2dp-source"
		description:	"High Fidelity Capture (A2DP Source)"
		priority:	19500
		available:	Id 2
		classes:
			String "Audio/Source"
"#;
        let ps = parse_profiles(text);
        assert_eq!(ps.len(), 3);
        assert_eq!(ps[0].index, 0);
        assert_eq!(ps[0].name, "off");
        assert!(!ps[0].available);
        assert!(ps[0].has_source);
        assert_eq!(ps[1].name, "a2dp-sink");
        assert!(ps[1].available);
        assert!(!ps[1].has_source);
        assert_eq!(ps[2].name, "a2dp-source");
        assert!(ps[2].available && ps[2].has_source);
    }

    #[test]
    fn pick_prefers_available_a2dp_source() {
        let ps = parse_profiles(ENUM_TEXT);
        assert_eq!(pick_receive_profile(&ps).unwrap().name, "a2dp-source");
    }

    #[test]
    fn pick_falls_back_to_duplex_then_other_source() {
        let duplex = make("a2dp-duplex", true, true);
        let headset = make("headset-head-unit", true, true);
        assert_eq!(
            pick_receive_profile(&[duplex.clone()]).unwrap().name,
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

    const ENUM_TEXT: &str = r#"	index:		0
		name:		"off"
		available:	Id 1
		classes:
			String "Audio/Sink"
		index:		1
		name:		"a2dp-sink"
		available:	Id 2
		classes:
			String "Audio/Sink"
		index:		2
		name:		"a2dp-source"
		available:	Id 2
		classes:
			String "Audio/Source"
"#;

    fn make(name: &str, available: bool, has_source: bool) -> Profile {
        Profile {
            index: 0,
            name: name.into(),
            available,
            has_source,
        }
    }
}
