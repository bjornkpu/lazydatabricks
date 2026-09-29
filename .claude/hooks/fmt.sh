#!/usr/bin/env bash
# PostToolUse hook: formats the crate after Claude edits a Rust file. Never fails the edit.
path=$(grep -o '"file_path"[[:space:]]*:[[:space:]]*"[^"]*"' | head -n 1)
case "$path" in
  *'.rs"') cd "${CLAUDE_PROJECT_DIR:-.}" && cargo fmt >/dev/null 2>&1 ;;
esac
exit 0
