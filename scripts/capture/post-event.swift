// post-event <pid> <step>... — post key presses to ONE process.
//
// Events go to <pid>'s own event queue (CGEvent.postToPid), never to whatever
// window happens to have focus, so a focus change cannot deliver a key to
// another app — or `q` to a PR Marmot instance under measurement.
//
// Steps:
//   key:<keycode>[+cmd][+shift][+alt][+ctrl]   e.g. key:125 (Down), key:49 (Space)
//   move:<x>,<y>                               the pointer to a screen point
//   scroll:<pixels>                            a wheel step down (negative: up)
//   wait:<milliseconds>
//
// No clicks: a posted mouse move reaches GPUI (hover shows), but a posted
// press does not make a GPUI click, and a real click could land in another
// window. A scene that needs a button needs a key for it. A move is posted
// to the process too, so only its window sees the hover; the real pointer
// does not travel.
import CoreGraphics
import Foundation

let args = Array(CommandLine.arguments.dropFirst())
guard args.count >= 2, let pid = pid_t(args[0]) else {
    FileHandle.standardError.write("usage: post-event <pid> <step>...\n".data(using: .utf8)!)
    exit(2)
}
let source = CGEventSource(stateID: .hidSystemState)
// Where the last move put the pointer: a wheel step is delivered there.
var pointer: CGPoint? = nil

func fail(_ message: String) -> Never {
    FileHandle.standardError.write("post-event: \(message)\n".data(using: .utf8)!)
    exit(2)
}

for step in args.dropFirst() {
    let parts = step.split(separator: ":", maxSplits: 1).map(String.init)
    guard parts.count == 2 else { fail("bad step \(step)") }
    switch parts[0] {
    case "key":
        let pieces = parts[1].split(separator: "+").map(String.init)
        guard let code = CGKeyCode(pieces[0]) else { fail("bad key \(step)") }
        var flags: CGEventFlags = []
        for modifier in pieces.dropFirst() {
            switch modifier {
            case "cmd": flags.insert(.maskCommand)
            case "shift": flags.insert(.maskShift)
            case "alt": flags.insert(.maskAlternate)
            case "ctrl": flags.insert(.maskControl)
            default: fail("bad modifier \(modifier)")
            }
        }
        for down in [true, false] {
            guard let event = CGEvent(keyboardEventSource: source, virtualKey: code, keyDown: down)
            else { fail("cannot make a key event") }
            event.flags = flags
            event.postToPid(pid)
            usleep(40_000)
        }
    case "move":
        let xy = parts[1].split(separator: ",").map(String.init)
        guard xy.count == 2, let x = Double(xy[0]), let y = Double(xy[1]) else {
            fail("bad move \(step)")
        }
        guard let event = CGEvent(
            mouseEventSource: source, mouseType: .mouseMoved,
            mouseCursorPosition: CGPoint(x: x, y: y), mouseButton: .left)
        else { fail("cannot make a mouse event") }
        event.postToPid(pid)
        pointer = CGPoint(x: x, y: y)
    case "scroll":
        guard let dy = Int32(parts[1]) else { fail("bad scroll \(step)") }
        guard let event = CGEvent(
            scrollWheelEvent2Source: source, units: .pixel, wheelCount: 1,
            wheel1: -dy, wheel2: 0, wheel3: 0)
        else { fail("cannot make a scroll event") }
        if let at = pointer { event.location = at }
        event.postToPid(pid)
    case "wait":
        guard let ms = UInt32(parts[1]) else { fail("bad wait \(step)") }
        usleep(ms * 1_000)
    default:
        fail("unknown step \(step)")
    }
    usleep(200_000)
}
