// A small AppKit application with KNOWN elements, for the macOS actuator's
// real-permission tests (PX-071). It is a background (accessory) app: it
// never takes focus. Every effect an actuator can have on it is written to a
// state file the test reads from OUTSIDE the accessibility API, so "the
// button was pressed" is proven by the application itself, not by the tree.
//
//   axfixture <state-file>
//
// <state-file>        key=value lines: ready, pressed, field, check, secure_len, window
// <state-file>.keys   one line per keyDown the application received: epoch ms
//
// The password field is pre-filled with a recognisable secret so a test can
// prove it never reaches a tree, a result or a frame.
import AppKit

let args = CommandLine.arguments
guard args.count >= 2 else {
    FileHandle.standardError.write(Data("usage: axfixture <state-file>\n".utf8))
    exit(2)
}
let stateURL = URL(fileURLWithPath: args[1])
let keysPath = args[1] + ".keys"
var state: [String: String] = [
    "ready": "0", "pressed": "0", "field": "", "check": "0", "secure_len": "0", "window": "open",
]
func flush() {
    let text = state.keys.sorted().map { "\($0)=\(state[$0] ?? "")" }.joined(separator: "\n") + "\n"
    try? text.write(to: stateURL, atomically: true, encoding: .utf8)
}
func logKey() {
    let ms = Date().timeIntervalSince1970 * 1000
    let line = String(format: "%.3f\n", ms)
    if let h = FileHandle(forWritingAtPath: keysPath) {
        h.seekToEndOfFile()
        h.write(Data(line.utf8))
        try? h.close()
    } else {
        try? line.write(toFile: keysPath, atomically: true, encoding: .utf8)
    }
}

final class Delegate: NSObject, NSApplicationDelegate, NSTextFieldDelegate, NSWindowDelegate {
    var window: NSWindow!
    var label: NSTextField!
    var field: NSTextField!
    var secure: NSSecureTextField!
    var count = 0

    func applicationDidFinishLaunching(_ n: Notification) {
        window = NSWindow(
            contentRect: NSRect(x: 220, y: 260, width: 480, height: 300),
            styleMask: [.titled, .closable], backing: .buffered, defer: false)
        window.title = "Modbit AX Fixture"
        window.delegate = self
        let content = window.contentView!

        label = NSTextField(labelWithString: "count=0")
        label.frame = NSRect(x: 20, y: 250, width: 200, height: 24)
        label.setAccessibilityIdentifier("count-label")
        content.addSubview(label)

        let button = NSButton(title: "Increment", target: self, action: #selector(pressed))
        button.frame = NSRect(x: 20, y: 200, width: 140, height: 32)
        content.addSubview(button)

        field = NSTextField(frame: NSRect(x: 20, y: 150, width: 300, height: 24))
        field.placeholderString = "type here"
        field.setAccessibilityLabel("Name")
        field.delegate = self
        content.addSubview(field)

        secure = NSSecureTextField(frame: NSRect(x: 20, y: 110, width: 300, height: 24))
        secure.stringValue = "hunter2-secret"
        secure.setAccessibilityLabel("Password")
        secure.delegate = self
        content.addSubview(secure)

        let check = NSButton(checkboxWithTitle: "Agree", target: self, action: #selector(toggled(_:)))
        check.frame = NSRect(x: 20, y: 70, width: 140, height: 24)
        content.addSubview(check)

        NSEvent.addLocalMonitorForEvents(matching: .keyDown) { e in
            logKey()
            return e
        }
        window.orderFrontRegardless()
        window.makeFirstResponder(field)
        state["ready"] = "1"
        state["secure_len"] = String(secure.stringValue.count)
        flush()
    }

    @objc func pressed() {
        count += 1
        label.stringValue = "count=\(count)"
        state["pressed"] = String(count)
        flush()
    }

    @objc func toggled(_ b: NSButton) {
        state["check"] = b.state == .on ? "1" : "0"
        flush()
    }

    func controlTextDidChange(_ n: Notification) {
        guard let f = n.object as? NSTextField else { return }
        if f === secure {
            state["secure_len"] = String(f.stringValue.count)
        } else {
            state["field"] = f.stringValue.replacingOccurrences(of: "\n", with: "\\n")
        }
        flush()
    }

    func windowWillClose(_ n: Notification) {
        state["window"] = "closed"
        flush()
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ s: NSApplication) -> Bool { false }
}

let app = NSApplication.shared
app.setActivationPolicy(.accessory)
let delegate = Delegate()
app.delegate = delegate
app.run()
