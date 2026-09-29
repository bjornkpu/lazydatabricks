#!/usr/bin/env bash
# Stop hook: the gate. Claude cannot end a turn while fmt, clippy or nextest is red.
#
# Skips when no build input changed since the last green run. When the gate is red and the tree
# is unchanged since the last block, Claude could not fix it: let the turn end and tell the user,
# instead of looping. A red gate on a changing tree blocks again each time; Claude stops editing
# eventually, and then the unchanged-tree rule ends the turn.
set -u
input=$(cat)
# Esc from the user always ends the turn.
printf '%s' "$input" | grep -q '"stop_reason"[[:space:]]*:[[:space:]]*"user_interrupt"' && exit 0
cd "${CLAUDE_PROJECT_DIR:-.}" || exit 0

sha=$(command -v sha1sum || command -v shasum)
state=target/.gate
mkdir -p "$state"
# -z: names with non-ASCII characters or spaces arrive unquoted.
hash=$(git ls-files -z -co --exclude-standard -- '*.rs' '*.snap' 'Cargo.toml' 'Cargo.lock' \
    'tests/*' 'clippy.toml' 'rustfmt.toml' '.rustfmt.toml' 'rust-toolchain.toml' '.cargo/*' \
  | sort -zu | xargs -0 -r "$sha" 2>/dev/null | "$sha" | cut -d' ' -f1)

# An empty hash means hashing failed: never skip on it.
if [ -n "$hash" ] && [ "$hash" = "$(cat "$state/green" 2>/dev/null)" ]; then
  rm -f "$state/red"
  exit 0
fi

out=$( { cargo fmt --check && cargo clippy --all-targets --all-features --quiet -- -D warnings \
  && cargo nextest run --all-features --no-tests=pass; } 2>&1 )
status=$?

if [ "$status" -eq 0 ]; then
  echo "$hash" > "$state/green"
  rm -f "$state/red"
  exit 0
fi

tail=$(printf '%s\n' "$out" | tail -n 40)
first=$(printf '%s\n' "$out" | grep -m 1 -E '^(error|FAIL|Diff in)' || printf '%s\n' "$tail" | head -n 1)
if [ -n "$hash" ] && [ "$hash" = "$(cat "$state/red" 2>/dev/null)" ]; then
  # Claude Code shows only the first stderr line of a non-blocking hook error, so it names the failure.
  printf 'GATE RED, unchanged since the last block: %s\n%s\n' "$first" "$tail" >&2
  exit 1
fi
echo "$hash" > "$state/red"
printf 'Gate red (fmt, clippy, nextest). Fix it before finishing:\n%s\n' "$tail" >&2
exit 2
