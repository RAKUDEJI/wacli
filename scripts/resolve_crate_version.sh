#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -ne 1 ]; then
  echo "usage: $0 <path-to-Cargo.toml>" >&2
  exit 2
fi

manifest="$1"

if [ ! -f "$manifest" ]; then
  echo "missing Cargo.toml: $manifest" >&2
  exit 2
fi

root_version="$(grep -m1 -E '^\s*version\s*=\s*"' Cargo.toml | sed -E 's/.*"([^"]+)".*/\1/')"

crate_version_line="$(grep -m1 -E '^\s*version(\.workspace)?\s*=' "$manifest" || true)"
if [ -z "$crate_version_line" ]; then
  echo "missing version in $manifest" >&2
  exit 2
fi

if echo "$crate_version_line" | grep -q "workspace"; then
  echo "$root_version"
  exit 0
fi

echo "$crate_version_line" | sed -E 's/.*"([^"]+)".*/\1/'
