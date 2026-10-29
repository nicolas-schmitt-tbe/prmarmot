// Native AX observations/actions for verify-appearance.sh. No global clicks,
// clipboard writes, application internals or screenshot-based assertions.
import ApplicationServices
import Foundation

func fail(_ message: String) -> Never {
    FileHandle.standardError.write(Data("appearance-ax: \(message)\n".utf8))
    exit(1)
}

let args = Array(CommandLine.arguments.dropFirst())
guard args.count >= 2, let pid = pid_t(args[0]), pid > 1 else {
    fail("usage: appearance-ax <pid> dump|press|press-prefix|set-value [name] [text]")
}
guard AXIsProcessTrusted() else {
    fail("Accessibility permission is required for the invoking terminal/runner")
}
let app = AXUIElementCreateApplication(pid)
AXUIElementSetMessagingTimeout(app, 2)

func attribute(_ element: AXUIElement, _ key: String) -> CFTypeRef? {
    var value: CFTypeRef?
    guard AXUIElementCopyAttributeValue(element, key as CFString, &value) == .success else {
        return nil
    }
    return value
}

func settable(_ element: AXUIElement, _ key: String) -> Bool {
    var result = DarwinBoolean(false)
    return AXUIElementIsAttributeSettable(element, key as CFString, &result) == .success
        && result.boolValue
}

func actions(_ element: AXUIElement) -> [String] {
    var names: CFArray?
    guard AXUIElementCopyActionNames(element, &names) == .success else { return [] }
    return names as? [String] ?? []
}

// Walk only this process's windows, not menus belonging to other applications.
// Keep traversal bounded and fail rather than returning a truncated assertion.
var elements: [AXUIElement] = []
func walk(_ element: AXUIElement, depth: Int) {
    guard depth < 64, elements.count < 8000 else { fail("AX tree exceeded its bound") }
    elements.append(element)
    for child in attribute(element, "AXChildren") as? [AXUIElement] ?? [] {
        walk(child, depth: depth + 1)
    }
}
for window in attribute(app, "AXWindows") as? [AXUIElement] ?? [] {
    walk(window, depth: 0)
}

let keys = ["AXRole", "AXSubrole", "AXTitle", "AXDescription", "AXHelp", "AXValue",
            "AXSelected", "AXFocused", "AXExpanded", "AXEnabled", "AXIdentifier"]

func matches(_ element: AXUIElement, _ name: String, prefix: Bool) -> Bool {
    ["AXTitle", "AXDescription", "AXValue"].contains { key in
        guard let value = attribute(element, key) as? String else { return false }
        return prefix ? value.hasPrefix(name) : value == name
    }
}

switch args[1] {
case "dump":
    let rows: [[String: Any]] = elements.map { element in
        var row: [String: Any] = [:]
        for key in keys {
            if let value = attribute(element, key), value is String || value is NSNumber {
                row[key] = value
            }
        }
        row["actions"] = actions(element)
        row["valueSettable"] = settable(element, "AXValue")
        return row
    }
    let data = try JSONSerialization.data(withJSONObject: rows, options: [.sortedKeys])
    FileHandle.standardOutput.write(data)
    print("")
case "press", "press-prefix":
    guard args.count == 3 else { fail("an exact AX name (or press-prefix) is required") }
    let found = elements.filter { element in
        guard matches(element, args[2], prefix: args[1] == "press-prefix") else { return false }
        return actions(element).contains("AXPress")
    }
    guard found.count == 1 else { fail("expected one actionable '\(args[2])', found \(found.count)") }
    let result = AXUIElementPerformAction(found[0], "AXPress" as CFString)
    guard result == .success else { fail("\(args[1]) '\(args[2])': AX error \(result.rawValue)") }
case "set-value":
    guard args.count == 4 else { fail("set-value requires editor name and text") }
    let found = elements.filter {
        matches($0, args[2], prefix: false) && settable($0, "AXValue")
    }
    guard found.count == 1 else { fail("expected one editable '\(args[2])', found \(found.count)") }
    let result = AXUIElementSetAttributeValue(found[0], "AXValue" as CFString, args[3] as CFString)
    guard result == .success else { fail("set-value: AX error \(result.rawValue)") }
default:
    fail("unknown command \(args[1])")
}
