#!/usr/bin/env bash
# capture.sh — screenshot PR Marmot's main screens on fictional demo data, in
# light and dark, the same way every time.
#
#   scripts/capture.sh [--out DIR] [--light | --dark] [--colors SET] [--skin SKIN]
#                      [--width POINTS] [--height POINTS] [--suffix TEXT]
#                      [--allow-same] [SCENE | NAME=STEPS ...]
#
# Scenes: board, my-prs, review-queue, all-open, pr-details, settings, selection, first,
# filter, bottom (default: all).
# Writes DIR/<scene>-<theme><suffix>.png (default DIR: target/captures). Uses the
# debug build, or PRMARMOT_DEMO_BINARY (e.g. target/release/prmarmot).
#
# Each scene is a fresh instance launched by scripts/demo.sh, so nothing a
# person has configured or signed in to is read, and nothing real is shown.
# The traps this avoids:
#   - GPUI draws nothing while its window is covered, so a capture would keep a
#     stale first frame. The window is brought to the front, and the scene
#     fails after 5 s if another window still lies over it.
#   - A key sent to "the focused window" can land anywhere; one once quit an
#     instance under a memory measurement. Keys here are posted to the
#     scene's own process (scripts/capture/post-event.swift), and nothing runs
#     while a memory-gate measurement is live.
#   - A frame taken mid-load or mid-transition: a scene is captured only after
#     the demo data has answered the board's query, and saved only once two
#     captures 0.6 s apart are identical.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="$ROOT/target/captures"
THEMES=(light dark)
WANTED=()
LOOK=()
SUFFIX=""
EXTRA=()
ALLOW_SAME=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    --out) OUT="${2:?--out needs a directory}"; shift ;;
    --light) THEMES=(light) ;;
    --dark) THEMES=(dark) ;;
    # The look and the window width go to demo.sh; the suffix names the files.
    --colors | --skin | --width | --height | --theme-file) LOOK+=("$1" "${2:?$1 needs a value}"); shift ;;
    --suffix) SUFFIX="${2:?--suffix needs text}"; shift ;;
    # A scene whose steps are meant to change nothing (a hover that must
    # show no hover) is not a failure.
    --allow-same) ALLOW_SAME=true ;;
    -h | --help) sed -n '2,9p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    --*) printf 'capture: unknown option %s\n' "$1" >&2; exit 2 ;;
    *=*) EXTRA+=("${1%%=*}||${1#*=}"); WANTED+=("${1%%=*}") ;;
    *) WANTED+=("$1") ;;
  esac
  shift
done

# name | app arguments | steps once the board has settled: a key code posted
# to the instance (key:125 is Down, key:49 Space, key:43+cmd ⌘,), ax:NAME, a
# click on the control whose accessibility name begins with NAME (+ for a
# space), or hover:X,Y, the pointer at that point of the window (from its
# top-left corner). Six Downs select #414, the first layer of the demo's
# stack; three select #415. Extra scenes can be given as NAME=STEPS on the
# command line.
SCENES=(
  "board||"
  # The README's My PRs image: one repository, so the view is My PRs.
  "my-prs|--repo demo-labs/atlas|"
  "review-queue|--review|"
  "all-open|--repo demo-labs/atlas --all-open|"
  "pr-details||key:125 key:125 key:125 key:125 key:125 key:125 key:49"
  "settings||key:43+cmd"
  "selection||key:125 key:125 key:125"
  # The first row, and (with --height 560) the last row, scrolled to.
  "first||key:125"
  # A quick filter on (Stale: one demo row).
  "filter||ax:Stale"
  "bottom||key:125 key:125 key:125 key:125 key:125 key:125 key:125 key:125 key:125 key:125 key:125 key:125 key:125 key:125 key:125 key:125 key:125 key:125 key:125 key:125 key:125 key:125 key:125 key:125 key:125 key:125 key:125 key:125 key:125 key:125"
  ${EXTRA[@]+"${EXTRA[@]}"}
)

if [[ $(uname) != Darwin ]]; then
  echo "capture: macOS only (screencapture, CGWindowList)" >&2
  exit 1
fi
if pgrep -f "scripts/(measure|measure-footprint|spike-run)\.sh" >/dev/null; then
  echo "capture: a memory-gate measurement is running; capture nothing until it ends" >&2
  exit 1
fi
BINARY="${PRMARMOT_DEMO_BINARY:-$ROOT/target/debug/prmarmot}"
if [[ ! -x "$BINARY" ]]; then
  printf 'capture: no binary at %s (cargo build first)\n' "$BINARY" >&2
  exit 1
fi

TOOLS="$(mktemp -d "${TMPDIR:-/tmp}/prmarmot-capture.XXXXXX")"
APP_PID=""
cleanup() {
  if [[ -n $APP_PID ]]; then kill "$APP_PID" 2>/dev/null || true; fi
  rm -rf "$TOOLS"
}
trap cleanup EXIT INT TERM
swiftc -O "$ROOT/scripts/capture/window.swift" -o "$TOOLS/window"
swiftc -O "$ROOT/scripts/capture/post-event.swift" -o "$TOOLS/post-event"
mkdir -p "$OUT"
# A sleeping display captures as black.
caffeinate -u -t 2 &

wanted() {
  [[ ${#WANTED[@]} -eq 0 ]] && return 0
  local name
  for name in "${WANTED[@]}"; do [[ $name == "$1" ]] && return 0; done
  return 1
}

# Bring the instance's window to the front and print "id x y w h", or fail
# after 5 s if it is still covered.
front_window() {
  local tries=0 info
  while ((tries < 25)); do
    osascript -e "tell application \"System Events\" to set frontmost of (first process whose unix id is $APP_PID) to true" >/dev/null 2>&1 || true
    if info="$("$TOOLS/window" "$APP_PID" 2>/dev/null)"; then
      echo "$info"
      return 0
    fi
    sleep 0.2
    tries=$((tries + 1))
  done
  "$TOOLS/window" "$APP_PID" >/dev/null || true # says why
  return 1
}

# Click the instance's control whose accessibility name begins with $1 (+ for
# a space), through the process itself, so no other window can be hit.
ax_click() {
  local name="${1//+/ }" tries=0
  # The accessibility tree fills in a moment after the window shows.
  while ! osascript - "$APP_PID" "$name" <<'OSA' >/dev/null 2>&1; do
on run {pid, prefix}
  tell application "System Events"
    tell (first process whose unix id is (pid as integer))
      set found to missing value
      repeat with e in (every UI element of UI element 1 of window 1)
        try
          set n to name of e
          if n is not missing value and (n as text) starts with prefix then
            set found to e
            exit repeat
          end if
        end try
      end repeat
      if found is missing value then error "no control named " & prefix
      click found
    end tell
  end tell
end run
OSA
    tries=$((tries + 1))
    if ((tries >= 10)); then
      echo "capture: no control named $name" >&2
      exit 1
    fi
    sleep 0.5
  done
  sleep 0.5
}

# Capture until two frames 0.6 s apart are identical (15 s at most).
settled_capture() {
  local wid="$1" out="$2" a="$TOOLS/a.png" b="$TOOLS/b.png" tries=0
  screencapture -x -o -l"$wid" "$a"
  while ((tries < 25)); do
    sleep 0.6
    screencapture -x -o -l"$wid" "$b"
    if cmp -s "$a" "$b"; then
      mv "$b" "$out"
      return 0
    fi
    mv "$b" "$a"
    tries=$((tries + 1))
  done
  echo "capture: the window never stopped changing" >&2
  return 1
}

captured=0
for theme in "${THEMES[@]}"; do
  for scene in "${SCENES[@]}"; do
    IFS='|' read -r name app_args steps <<<"$scene"
    wanted "$name" || continue
    flags=(${LOOK[@]+"${LOOK[@]}"})
    [[ $theme == dark ]] && flags+=(--dark)
    answered="$TOOLS/$name-$theme.answered"
    # shellcheck disable=SC2206 # app_args are fixed words from the table above
    args=($app_args)
    PRMARMOT_DEMO_BINARY="$BINARY" PRMARMOT_DEMO_ANSWERED="$answered" "$ROOT/scripts/demo.sh" ${flags[@]+"${flags[@]}"} \
      ${args[@]+"${args[@]}"} >"$TOOLS/$name-$theme.log" 2>&1 &
    launcher=$!
    APP_PID=""
    for _ in $(seq 50); do
      APP_PID="$(pgrep -P "$launcher" | head -1 || true)"
      [[ -n $APP_PID ]] && break
      sleep 0.2
    done
    if [[ -z $APP_PID ]]; then
      echo "capture: $name ($theme) did not start" >&2
      cat "$TOOLS/$name-$theme.log" >&2
      exit 1
    fi
    for _ in $(seq 75); do
      [[ -s $answered ]] && break
      sleep 0.2
    done
    if [[ ! -s $answered ]]; then
      echo "capture: $name ($theme): the board never asked for its pull requests" >&2
      cat "$TOOLS/$name-$theme.log" >&2
      exit 1
    fi
    read -r wid _ _ _ _ < <(front_window) || {
      echo "capture: $name ($theme): window covered or missing" >&2
      exit 1
    }
    settled_capture "$wid" "$TOOLS/before.png"
    if [[ -n $steps ]]; then
      for step in $steps; do
        case "$step" in
          ax:*) ax_click "${step#ax:}" ;;
          hover:*)
            # The window's own corner, so a scene names a point of the window.
            read -r _ wx wy _ _ < <(front_window)
            IFS=, read -r hx hy <<<"${step#hover:}"
            "$TOOLS/post-event" "$APP_PID" "move:$((wx + hx)),$((wy + hy))"
            ;;
          # shellcheck disable=SC2086 # steps are fixed words from the table above
          *) "$TOOLS/post-event" "$APP_PID" $step ;;
        esac
      done
      read -r wid _ _ _ _ < <(front_window) || {
        echo "capture: $name ($theme): window covered after input" >&2
        exit 1
      }
    fi
    settled_capture "$wid" "$OUT/$name-$theme$SUFFIX.png"
    if [[ -n $steps ]] && ! $ALLOW_SAME && cmp -s "$TOOLS/before.png" "$OUT/$name-$theme$SUFFIX.png"; then
      echo "capture: $name ($theme): the input changed nothing on screen" >&2
      exit 1
    fi
    echo "$OUT/$name-$theme$SUFFIX.png"
    captured=$((captured + 1))
    kill "$APP_PID" 2>/dev/null || true
    wait "$launcher" 2>/dev/null || true
    APP_PID=""
  done
done

if ((captured == 0)); then
  echo "capture: no scene matched ${WANTED[*]}" >&2
  exit 2
fi
