#!/usr/bin/env bash
# Native behavioral contract for reversible Settings appearance previews.
# Usage: PRMARMOT_DEMO_BINARY=target/debug/prmarmot scripts/verify-appearance.sh
#        scripts/verify-appearance.sh [--require-reviewer] [--timeout SECONDS]
#
# macOS + Accessibility permission for the terminal/runner are required. Uses
# demo.sh's fictional data and a private config, never an installed app. Builds
# only the small native input/AX helpers, NOT the app. Do not run during a G0
# measurement. No screenshots, global keystrokes, clipboard or app test hooks.
#
# Assertions: AX toggle values and selected PR/filter across preview changes;
# Cancel/Escape restore both directions without even rewriting config; Escape
# closes Colours before preview; Back preserves an editable reviewer draft;
# Save persists skin/theme, with unrelated TOML intact, and reopened Settings
# reads the saved choice. Polls observable conditions, never elapsed time as
# evidence. An unsupported reviewer editor is explicitly SKIP (strict with
# --require-reviewer). A missing appearance/selection AX contract is a failure.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [[ ${1:-} == -h || ${1:-} == --help ]]; then
  sed -n '2,17p' "$0" | sed 's/^# \{0,1\}//'
  exit 0
fi
if [[ $(uname) != Darwin ]]; then
  echo "verify-appearance: macOS only" >&2
  exit 1
fi
if pgrep -f '(^|[ /])(measure|measure-footprint|spike-run)\.sh([[:space:]]|$)' >/dev/null; then
  echo "verify-appearance: memory-gate process running; refusing to launch or send input" >&2
  exit 1
fi

exec python3 - "$ROOT" "$@" <<'PY'
import argparse
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time

try:
    import tomllib
except ImportError:
    sys.exit("verify-appearance: Python 3.11+ is required (stdlib tomllib)")

root = Path(sys.argv[1])
parser = argparse.ArgumentParser(description="Native appearance-preview integration check")
parser.add_argument("--require-reviewer", action="store_true")
parser.add_argument("--timeout", type=float, default=15)
args = parser.parse_args(sys.argv[2:])
if not 1 <= args.timeout <= 120:
    parser.error("--timeout must be between 1 and 120 seconds")
binary = Path(os.environ.get("PRMARMOT_DEMO_BINARY", root / "target/debug/prmarmot")).expanduser().resolve()
if "Applications" in binary.parts or ".app" in str(binary):
    sys.exit("verify-appearance: use a standalone build, never an installed/bundled app")
if not binary.is_file() or not os.access(binary, os.X_OK):
    sys.exit(f"verify-appearance: no executable at {binary}; build the app separately first")


def run(*command, **kwargs):
    return subprocess.run([str(p) for p in command], text=True, capture_output=True,
                          timeout=10, check=True, **kwargs).stdout.strip()


def guard_gate():
    result = subprocess.run(
        ["pgrep", "-f", r"(^|[ /])(measure|measure-footprint|spike-run)\.sh([[:space:]]|$)"],
        capture_output=True, text=True, timeout=5)
    if result.returncode != 1:
        raise RuntimeError("memory-gate process detected (or pgrep failed); refusing further input")


def wait_for(description, check):
    deadline = time.monotonic() + args.timeout
    while True:
        guard_gate()
        result = check()
        if result:
            return result
        if time.monotonic() >= deadline:
            raise AssertionError(f"timed out: {description}")
        time.sleep(0.1)  # Poll cadence only: success always needs the observation above.


def names(node):
    return [node[k] for k in ("AXTitle", "AXDescription") if isinstance(node.get(k), str)]


def named(nodes, name, prefix=False):
    return [n for n in nodes if any(v.startswith(name) if prefix else v == name for v in names(n))]


def control(nodes, name, prefix=False):
    candidates = [n for n in named(nodes, name, prefix) if "AXPress" in n.get("actions", [])]
    if len(candidates) > 1:
        raise AssertionError(f"ambiguous AX control {name!r}: {candidates}")
    return candidates[0] if candidates else None


def toggled(node, expected):
    # AXValue on a toggle button is numeric; missing/string values must not
    # accidentally prove an unselected control (e.g. bool(None) == False).
    return node is not None and isinstance(node.get("AXValue"), (bool, int)) \
        and node["AXValue"] == int(expected)


def appearance(nodes, skin, theme):
    return all(toggled(control(nodes, name + " skin"), name.lower() == skin)
               for name in ("Classic", "Glacier")) and all(
        toggled(control(nodes, name), name.lower() == theme) for name in ("System", "Light", "Dark"))


def board_skin(nodes, skin):
    # Observe the board itself, not just its Settings draft or config: Classic
    # renders native tabs; Glacier renders toggle buttons in its raised track.
    # Reopening Settings alone cannot prove a cancelled live skin was restored.
    switch = control(nodes, "Review queue")
    return switch is not None and switch.get("AXRole") == (
        "AXRadioButton" if skin == "classic" else "AXCheckBox")


def config_values(data):
    # save_settings materializes these existing defaults and removes skin when
    # returning to Classic. Compare their meaning, while preserving every other
    # key/table; Cancel is stricter and checks bytes + filesystem metadata.
    return dict({"skin": "classic", "notifications": True, "notification_sound": True,
                 "notify_all_needs_action": False, "dock_badge": True}, **data)


def editor(nodes):
    fields = [n for n in named(nodes, "Reviewer suggestions")
              if n.get("AXRole") in ("AXTextField", "AXTextArea")]
    if len(fields) > 1:
        raise AssertionError("ambiguous reviewer editor")
    return fields[0] if fields else None


def board_value(nodes):
    rows = [n for n in nodes if n.get("AXRole") == "AXRow"
            and any(v.startswith("Pull request #") for v in names(n))]
    row_name = lambda n: next(v for v in names(n) if v.startswith("Pull request #"))
    # AccessKit's macOS adapter does not expose AXSelected for these rows.
    # The open Details heading names the actual selected PR; if selection is
    # lost, the pane displays its empty instruction instead. Keep it open.
    selected = sorted({value for n in nodes for value in names(n) + [n.get("AXValue")]
                       if isinstance(value, str) and value.startswith("#410  ")})
    return {"rows": sorted(row_name(n) for n in rows),
            "selected": selected,
            "stale": toggled(control(nodes, "Stale", prefix=True), True)}


launcher = None
app_pid = None
latest = []
skips = []


def interrupted(signum, _frame):
    raise RuntimeError(f"interrupted by signal {signum}")


signal.signal(signal.SIGTERM, interrupted)
signal.signal(signal.SIGINT, interrupted)

with tempfile.TemporaryDirectory(prefix="prmarmot-verify-appearance.") as temp:
    work = Path(temp).resolve()  # macOS /var is a symlink to /private/var.
    config = None
    log = work / "app.log"
    try:
        guard_gate()
        for source, target in (("post-event.swift", "post-event"), ("appearance-ax.swift", "ax")):
            subprocess.run(["swiftc", "-O", str(root / "scripts/capture" / source),
                            "-o", str(work / target)], check=True)
        # demo.sh still owns config creation/deletion. Pin the ordinary auth
        # settings to fake gh + private file storage (Settings otherwise reads
        # the real Keychain even when gh is signed in). Disable update checks.
        # The handshake records process/config identity, not app internals.
        wrapper = work / "launch"
        wrapper.write_text('''#!/bin/sh
set -eu
config="$XDG_CONFIG_HOME/prmarmot/config.toml"
sed 's/^automatic_update_checks = true$/automatic_update_checks = false/' "$config" > "$config.tmp"
cat >> "$config.tmp" <<'AUTH'

[auth]
host = "github.com"
mode = "gh"
store = "file"
AUTH
mv "$config.tmp" "$config"
printf '%s\\n' "$$" "$XDG_CONFIG_HOME" > "$PRMARMOT_VERIFY_HANDSHAKE"
exec "$PRMARMOT_VERIFY_BINARY" "$@"
''')
        wrapper.chmod(0o700)
        handshake = work / "child"
        answered = work / "answered"
        env = os.environ.copy()
        for name in list(env):
            if name.startswith("PRMARMOT_") or name in (
                    "GH_TOKEN", "GITHUB_TOKEN", "GH_ENTERPRISE_TOKEN", "GITHUB_ENTERPRISE_TOKEN", "GH_HOST"):
                env.pop(name)
        env.update(PRMARMOT_DEMO_BINARY=str(wrapper), PRMARMOT_VERIFY_BINARY=str(binary),
                   PRMARMOT_VERIFY_HANDSHAKE=str(handshake), PRMARMOT_DEMO_ANSWERED=str(answered),
                   TMPDIR=str(work))
        guard_gate()
        with log.open("w") as output:
            launcher = subprocess.Popen([str(root / "scripts/demo.sh"), "--skin", "classic",
                                         "--width", "1440", "--height", "900"],
                                        env=env, stdout=output, stderr=subprocess.STDOUT,
                                        start_new_session=True)

        def started():
            if launcher.poll() is not None:
                raise AssertionError("demo launcher exited before app startup")
            return handshake.read_text().splitlines() if handshake.exists() else []

        child = wait_for("demo child handshake", lambda: (v if len(v := started()) == 2 else None))
        app_pid = int(child[0])
        config = Path(child[1]).resolve() / "prmarmot/config.toml"
        if not config.is_relative_to(work) or app_pid <= 1:
            raise AssertionError("demo escaped its private temp directory")

        def alive():
            if launcher.poll() is not None:
                raise AssertionError("demo app/launcher exited unexpectedly")
            if run("ps", "-p", app_pid, "-o", "ppid=") != str(launcher.pid):
                raise AssertionError("target is no longer our demo child")

        def native(command, *values):
            guard_gate()
            alive()
            return run(work / "ax", app_pid, command, *values)

        def snapshot():
            global latest
            latest = json.loads(native("dump"))
            return latest

        def expect(description, predicate):
            return wait_for(description, lambda: predicate(snapshot()))

        def key(step):
            guard_gate()
            alive()
            run("osascript", "-e", 'tell application "System Events" to set frontmost of '
                f'(first process whose unix id is {app_pid}) to true')
            run(work / "post-event", app_pid, step)

        def click(name, prefix=False):
            expect(f"actionable {name}", lambda nodes: control(nodes, name, prefix))
            native("press-prefix" if prefix else "press", name)

        def file_state():
            stat = config.stat()
            return (config.read_bytes(), stat.st_ino, stat.st_mtime_ns, stat.st_ctime_ns)

        def unchanged():
            if file_state() != baseline_file:
                raise AssertionError("preview/cancel rewrote config (contents, inode, mtime or ctime changed)")
            if answered.read_text() != baseline_reads:
                raise AssertionError("appearance preview fetched the board again")

        def board_kept():
            expect("same selected PR and Stale-filtered rows", lambda n: board_value(n) == baseline_board)

        def open_settings(skin, theme):
            expect(f"live board renders {skin}", lambda n: board_skin(n, skin))
            key("key:43+cmd")
            expect("Settings open", lambda n: control(n, "Preview on board"))
            expect(f"Settings says {skin}/{theme}", lambda n: appearance(n, skin, theme))

        def preview():
            click("Preview on board")
            expect("board preview open", lambda n: control(n, "Back to Settings")
                   and not control(n, "Preview on board"))
            board_kept()
            unchanged()

        def choose(skin, theme):
            click(skin.title() + " skin")
            click(theme.title())
            expect(f"AX toggles say {skin}/{theme}", lambda n: appearance(n, skin, theme))
            expect(f"live board renders {skin}", lambda n: board_skin(n, skin))
            board_kept()
            unchanged()

        def closed():
            expect("Settings and preview closed", lambda n: not control(n, "Back to Settings")
                   and not control(n, "Preview on board") and not control(n, "Save")
                   and control(n, "Settings"))
            board_kept()

        def colors_picker(nodes):
            matches = [n for n in nodes if n.get("AXRole") in ("AXPopUpButton", "AXComboBox")
                       and n.get("AXValue") == "Classic"]
            if len(matches) > 1:
                raise AssertionError("ambiguous Classic Colours selector")
            return matches[0] if matches else None

        def cancel_case(skin, theme, other_skin, other_theme, escape=False):
            open_settings(skin, theme)
            preview()
            choose(other_skin, other_theme)
            if escape:
                expect("Colours picker", colors_picker)
                native("press", "Colours")
                key("key:53")
                # This adapter exports neither AXExpanded nor option labels.
                # If the popup had not opened, this Escape would cancel the
                # preview. The two Escapes must have different outcomes.
                expect("first Escape leaves preview open", lambda n: colors_picker(n)
                       and control(n, "Back to Settings") and appearance(n, other_skin, other_theme))
                board_kept()
                unchanged()
                key("key:53")
            else:
                click("Cancel")
            closed()
            unchanged()
            open_settings(skin, theme)
            click("Cancel")
            closed()
            unchanged()
            print(f"ok - {'popup Escape then preview Escape' if escape else 'Cancel'} restores {skin}/{theme}; config untouched")

        def save_case(skin, theme, other_skin, other_theme):
            global baseline_file
            before = config_values(tomllib.loads(baseline_file[0].decode()))
            open_settings(skin, theme)
            preview()
            choose(other_skin, other_theme)
            click("Save")
            closed()
            expected = dict(before, skin=other_skin, theme=other_theme)
            wait_for("saved skin/theme and all unrelated TOML preserved",
                     lambda: config_values(tomllib.loads(config.read_text())) == expected)
            if file_state() == baseline_file:
                raise AssertionError("Save did not write config")
            baseline_file = file_state()
            open_settings(other_skin, other_theme)
            click("Cancel")
            closed()
            unchanged()
            print(f"ok - Save persists {other_skin}/{other_theme}; reopened Settings agrees; unrelated config retained")

        wait_for("fictional GraphQL answer", lambda: answered.exists() and answered.stat().st_size > 0)
        guard_gate()
        alive()
        run("osascript", "-e", 'tell application "System Events" to set frontmost of '
            f'(first process whose unix id is {app_pid}) to true')
        expect("fictional board ready", lambda n: len(board_value(n)["rows"]) > 1)
        click("Stale", prefix=True)
        expect("Stale filters to one fixture PR", lambda n: board_value(n)["stale"]
               and len(board_value(n)["rows"]) == 1)
        key("key:125")  # filter_by returns focus to the table; Down selects its lone PR.
        key("key:49")  # Keep Details open as the observable selected-PR identity.
        expect("one selected filtered PR", lambda n: len(board_value(n)["selected"]) == 1)
        baseline_board = board_value(snapshot())
        baseline_file = file_state()
        baseline_reads = answered.read_text()
        print(f"ok - isolated demo PID {app_pid}; board baseline {json.dumps(baseline_board)}")

        open_settings("classic", "light")
        field = editor(snapshot())
        original_reviewers = field.get("AXValue") if field else None
        draft = "alex, preview-reviewer"
        editing = field is not None and field.get("valueSettable")
        if editing:
            native("set-value", "Reviewer suggestions", draft)
            expect("reviewer draft edited through native input", lambda n: (editor(n) or {}).get("AXValue") == draft)
        else:
            reason = "reviewer draft: no AX-editable Reviewer suggestions text field"
            if args.require_reviewer:
                raise AssertionError(reason)
            skips.append(reason)
            print(f"SKIP - {reason}")
        preview()
        choose("glacier", "dark")
        choose("classic", "light")
        choose("glacier", "dark")
        click("Back to Settings")
        expect("Back returns to same Settings draft", lambda n: control(n, "Preview on board")
               and appearance(n, "glacier", "dark"))
        if editing:
            expect("Back retained edited reviewer text", lambda n: (editor(n) or {}).get("AXValue") == draft)
        unchanged()
        preview()
        click("Cancel")
        closed()
        unchanged()
        open_settings("classic", "light")
        if editing:
            expect("Cancel discarded reviewer draft", lambda n: (editor(n) or {}).get("AXValue") == original_reviewers)
        click("Cancel")
        closed()
        unchanged()
        print("ok - repeated skin/theme switches and Back preserve the draft; Cancel restores the original")

        open_settings("classic", "light")
        click("Glacier skin")
        click("Dark")
        key("key:53")
        closed()
        unchanged()
        open_settings("classic", "light")
        preview()
        native("press", "Colours")
        key("key:125")
        key("key:36")
        expect("keyboard picks Studio colours", lambda n: (control(n, "Colours") or {}).get("AXValue") == "Studio")
        board_kept()
        unchanged()
        click("Cancel")
        closed()
        open_settings("classic", "light")
        expect("Cancel restored Classic colours", colors_picker)
        click("Cancel")
        closed()
        unchanged()
        print("ok - dialog Escape and colour-picker keyboard selection roll back without fetching")

        cancel_case("classic", "light", "glacier", "dark", escape=True)
        save_case("classic", "light", "glacier", "dark")
        # Prevent a hard-coded 'restore Classic/Light' implementation passing.
        cancel_case("glacier", "dark", "classic", "light")
        cancel_case("glacier", "dark", "classic", "light", escape=True)
        save_case("glacier", "dark", "classic", "light")
        key("key:17")  # `t` persists the dark preference outside Settings.
        wait_for("theme shortcut saved dark", lambda: tomllib.loads(config.read_text())["theme"] == "dark")
        baseline_file = file_state()
        open_settings("classic", "dark")
        preview()
        choose("glacier", "light")
        click("System")
        expect("System appearance selected", lambda n: appearance(n, "glacier", "system"))
        click("Cancel")
        closed()
        unchanged()
        open_settings("classic", "dark")
        click("Cancel")
        closed()
        print("ok - System preview cancels back to the appearance chosen with the keyboard")
        print(f"PASS - appearance contract ({len(skips)} explicitly skipped optional checks)")
    except Exception as error:
        print(f"FAIL - {error}", file=sys.stderr)
        if isinstance(error, subprocess.CalledProcessError):
            print(error.stderr or error.stdout or "", file=sys.stderr)
        if log.exists():
            print("--- own demo log ---\n" + log.read_text(errors="replace")[-6000:], file=sys.stderr)
        print("--- last AX observation ---\n" + json.dumps(latest, indent=2), file=sys.stderr)
        sys.exit(1)
    finally:
        # Only this run's process group, never name-based pkill or installed apps.
        # Let demo.sh reap its child and delete its config normally first.
        signal.signal(signal.SIGINT, signal.SIG_IGN)
        signal.signal(signal.SIGTERM, signal.SIG_IGN)
        if launcher is not None and launcher.poll() is None:
            if app_pid is not None:
                try:
                    if os.getpgid(app_pid) == launcher.pid:
                        os.kill(app_pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
            try:
                launcher.wait(timeout=5)
            except subprocess.TimeoutExpired:
                os.killpg(launcher.pid, signal.SIGTERM)
                try:
                    launcher.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    os.killpg(launcher.pid, signal.SIGKILL)
                    launcher.wait(timeout=5)
PY
