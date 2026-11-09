// A small AppKit application with KNOWN elements, for the macOS actuator's
// real-permission tests (PX-071). It is a background (accessory) app: it
// never takes focus. Every effect an actuator can have on it is written to a
// state file the test reads from OUTSIDE the accessibility API, so "the
// button was pressed" is proven by the application itself, not by the tree.
//
//   axfixture <state-file> [form|document|dialog]
//
// The mode picks one of the fixture application suite's applications (each is
// its own bundle, built from this one source by `tests/common/fixture_app.rs`):
//   form      (default) a counter, a Name field, a password field, a checkbox
//   document  a multi-line text document with a Save button
//   dialog    a Delete... button that opens a modal sheet (Delete / Cancel)
//
// <state-file>        key=value lines: ready, pressed, field, check, secure_len, window,
//                     and in document mode doc, saved; in dialog mode sheet, dialog
// <state-file>.keys   one line per keyDown the application received: epoch ms, a space,
//                     the key's characters (ignoring modifiers)
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
let mode = args.count >= 3 ? args[2] : "form"
guard ["form", "document", "dialog"].contains(mode) else {
    FileHandle.standardError.write(Data("axfixture: unknown mode \(mode)\n".utf8))
    exit(2)
}
let keysPath = args[1] + ".keys"
var state: [String: String] = [
    "ready": "0", "pressed": "0", "field": "", "check": "0", "secure_len": "0", "window": "open",
]
func flush() {
    let text = state.keys.sorted().map { "\($0)=\(state[$0] ?? "")" }.joined(separator: "\n") + "\n"
    try? text.write(to: stateURL, atomically: true, encoding: .utf8)
}
func logKey(_ e: NSEvent) {
    let ms = Date().timeIntervalSince1970 * 1000
    let chars = (e.charactersIgnoringModifiers ?? "").replacingOccurrences(of: "\n", with: " ")
    let line = String(format: "%.3f", ms) + " " + chars + "\n"
    if let h = FileHandle(forWritingAtPath: keysPath) {
        h.seekToEndOfFile()
        h.write(Data(line.utf8))
        try? h.close()
    } else {
        try? line.write(toFile: keysPath, atomically: true, encoding: .utf8)
    }
}

final class Delegate: NSObject, NSApplicationDelegate, NSTextFieldDelegate, NSWindowDelegate,
    NSTextViewDelegate
{
    var window: NSWindow!
    var label: NSTextField!
    var field: NSTextField!
    var secure: NSSecureTextField!
    var count = 0
    var doc: NSTextView!
    var saves = 0
    var status: NSTextField!

    func applicationDidFinishLaunching(_ n: Notification) {
        NSEvent.addLocalMonitorForEvents(matching: .keyDown) { e in
            logKey(e)
            return e
        }
        switch mode {
        case "document": buildDocument()
        case "dialog": buildDialog()
        default: buildForm()
        }
    }

    func newWindow(_ title: String) -> NSView {
        window = NSWindow(
            contentRect: NSRect(x: 220, y: 260, width: 480, height: 300),
            styleMask: [.titled, .closable], backing: .buffered, defer: false)
        window.title = title
        window.delegate = self
        return window.contentView!
    }

    func show(_ first: NSView?) {
        window.orderFrontRegardless()
        if let f = first { window.makeFirstResponder(f) }
        state["ready"] = "1"
        flush()
    }

    // The document application: one multi-line text area and Save.
    func buildDocument() {
        let content = newWindow("Modbit AX Document")
        let scroll = NSScrollView(frame: NSRect(x: 20, y: 70, width: 440, height: 210))
        scroll.hasVerticalScroller = true
        doc = NSTextView(frame: scroll.contentView.bounds)
        doc.isRichText = false
        doc.autoresizingMask = [.width]
        doc.setAccessibilityLabel("Document")
        doc.delegate = self
        scroll.documentView = doc
        content.addSubview(scroll)
        let save = NSButton(title: "Save", target: self, action: #selector(saved))
        save.frame = NSRect(x: 20, y: 20, width: 140, height: 32)
        content.addSubview(save)
        state["doc"] = ""
        state["saved"] = "0"
        show(doc)
    }

    func textDidChange(_ n: Notification) {
        state["doc"] = doc.string.replacingOccurrences(of: "\n", with: "\\n")
        flush()
    }

    @objc func saved() {
        saves += 1
        state["saved"] = String(saves)
        state["doc"] = doc.string.replacingOccurrences(of: "\n", with: "\\n")
        flush()
    }

    // The modal-dialog application: Delete... opens a sheet that blocks the window.
    func buildDialog() {
        let content = newWindow("Modbit AX Dialog")
        status = NSTextField(labelWithString: "item=present")
        status.frame = NSRect(x: 20, y: 250, width: 300, height: 24)
        status.setAccessibilityIdentifier("item-label")
        content.addSubview(status)
        let del = NSButton(title: "Delete\u{2026}", target: self, action: #selector(askDelete))
        del.frame = NSRect(x: 20, y: 200, width: 140, height: 32)
        content.addSubview(del)
        state["sheet"] = "closed"
        state["dialog"] = "none"
        show(nil)
    }

    @objc func askDelete() {
        let alert = NSAlert()
        alert.messageText = "Delete the item?"
        alert.informativeText = "This cannot be undone."
        alert.addButton(withTitle: "Delete")
        alert.addButton(withTitle: "Cancel")
        state["sheet"] = "open"
        flush()
        alert.beginSheetModal(for: window) { r in
            let deleted = r == .alertFirstButtonReturn
            state["dialog"] = deleted ? "deleted" : "cancelled"
            state["sheet"] = "closed"
            if deleted { self.status.stringValue = "item=deleted" }
            flush()
        }
    }

    // The form application (the default; unchanged).
    func buildForm() {
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

        state["secure_len"] = String(secure.stringValue.count)
        show(field)
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
