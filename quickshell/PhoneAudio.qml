// PhoneAudio.qml — quickshell shim for phone-audio.
//
// Polls `phone-audio status --json` once per second through the CLI binary
// (PATH must include ~/.local/bin) and exposes the parsed state as properties
// a bar panel can bind to. Outgoing actions go through the CLI as well, so the
// shim duplicates none of the core logic.
//
//     PhoneAudio.summary      "Maxii · on · 65%"
//     PhoneAudio.available    is a bluetooth phone present
//     PhoneAudio.on           is the loopback running
//     PhoneAudio.phone        selected phone display name
//     PhoneAudio.profile      active bluez profile
//     PhoneAudio.volume       volume 0-100
//     PhoneAudio.reason       human note when something needs attention
//     PhoneAudio.toggle()     flip the routing on/off

pragma Singleton
import QtQuick
import Quickshell.Io

QtObject {
    id: root

    readonly property bool available: root._json.available === true
    readonly property bool on: root._json.on === true
    readonly property string phone: (root._json.phone && root._json.phone.name) || ""
    readonly property string profile: root._json.profile || ""
    readonly property int volume: root._json.volume === null ? -1 : Math.round(root._json.volume)
    readonly property string reason: root._json.reason || ""
    readonly property string summary: {
        if (!root.available) return "no phone"
        var text = root.phone + " · " + (root.on ? "on" : "off")
        if (root.volume >= 0) text += " · " + root.volume + "%"
        if (root.reason) text += " (" + root.reason + ")"
        return text
    }
    property var _json: ({})

    // Run the CLI query once per second.
    Timer {
        interval: 1000
        running: true
        repeat: true
        onTriggered: root.refresh()
    }

    Process {
        id: query
        command: ["phone-audio", "status", "--json"]
        stdout: StdioCollector {
            id: collector
        }
        onExited: {
            var text = collector.text.trim()
            try { root._json = JSON.parse(text) } catch (err) { root._json = {} }
        }
    }

    Process {
        id: action
        command: ["phone-audio", "toggle"]
        stdinEnabled: false
    }

    function refresh() { if (!query.running) query.running = true }
    function toggle() { if (!action.running) action.running = true }
}