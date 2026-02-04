#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -ne 2 ]; then
  echo "usage: $0 <crate-name> <path-to-Cargo.toml>" >&2
  exit 2
fi

crate="$1"
manifest="$2"
tag="${TAG:-}"

if [ -z "$tag" ]; then
  echo "TAG is required" >&2
  exit 2
fi

version="$(scripts/resolve_crate_version.sh "$manifest")"

if [ "v${version}" != "$tag" ]; then
  echo "skip publish: tag ${tag} != v${version}"
  exit 0
fi

if cargo info "$crate" --registry crates-io 2>&1 | grep -q "version: ${version}"; then
  echo "already published"
  exit 0
fi

if cargo publish -p "$crate"; then
  exit 0
fi

if cargo info "$crate" --registry crates-io 2>&1 | grep -q "version: ${version}"; then
  echo "already published"
  exit 0
fi

exit 1
