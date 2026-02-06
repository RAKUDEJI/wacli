#!/usr/bin/env python3

from __future__ import annotations

import argparse
import datetime as _dt
import hashlib
import json
import subprocess
import sys


def _sha256_digest(path: str) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        while True:
            chunk = f.read(1024 * 1024)
            if not chunk:
                break
            h.update(chunk)
    return "sha256:" + h.hexdigest()


def _parse_world_root_imports_exports(wit_text: str) -> tuple[list[str], list[str]]:
    inside = False
    imports: list[str] = []
    exports: list[str] = []
    for line in wit_text.splitlines():
        s = line.strip()
        if s == "world root {":
            inside = True
            continue
        if inside and s == "}":
            break
        if not inside:
            continue
        if s.startswith("import "):
            imports.append(s[len("import ") :].rstrip(";").strip())
        elif s.startswith("export "):
            exports.append(s[len("export ") :].rstrip(";").strip())
    return imports, exports


def _guess_os(imports: list[str], exports: list[str]) -> str:
    # Molt's registry search supports wasip1|wasip2; guess based on WIT interfaces.
    if any(s.startswith("wasi:") for s in (imports + exports)):
        return "wasip2"
    return "wasip1"


def _utc_now_iso_z() -> str:
    return _dt.datetime.now(_dt.timezone.utc).isoformat().replace("+00:00", "Z")


def main() -> int:
    ap = argparse.ArgumentParser(
        description="Generate OCI wasm config (application/vnd.wasm.config.v0+json) for a .component.wasm layer."
    )
    ap.add_argument("component", help="Path to *.component.wasm")
    ap.add_argument(
        "--created",
        default=None,
        help="RFC3339/ISO8601 timestamp. Defaults to current UTC time.",
    )
    ap.add_argument(
        "--os",
        dest="os_override",
        default=None,
        help="Override os (wasip1|wasip2). Defaults to a best-effort guess from WIT.",
    )
    ap.add_argument(
        "--target",
        default=None,
        help="Optional component target world/interface (indexed by some registries).",
    )
    args = ap.parse_args()

    wit = subprocess.check_output(
        ["wasm-tools", "component", "wit", args.component], text=True
    )
    imports, exports = _parse_world_root_imports_exports(wit)

    os_name = args.os_override or _guess_os(imports, exports)
    created = args.created or _utc_now_iso_z()
    layer_digest = _sha256_digest(args.component)

    cfg: dict[str, object] = {
        "created": created,
        "architecture": "wasm",
        "os": os_name,
        "layerDigests": [layer_digest],
        "component": {
            "exports": exports,
            "imports": imports,
        },
    }
    if args.target:
        assert isinstance(cfg["component"], dict)
        cfg["component"]["target"] = args.target

    json.dump(cfg, sys.stdout, separators=(",", ":"))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

