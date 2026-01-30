# wacli

WebAssembly Component composition CLI tool.

[![CI](https://github.com/RAKUDEJI/wacli/actions/workflows/ci.yml/badge.svg)](https://github.com/RAKUDEJI/wacli/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/wacli.svg)](https://crates.io/crates/wacli)
[![License](https://img.shields.io/crates/l/wacli.svg)](LICENSE)

## Overview

wacli is a CLI tool for composing WebAssembly Components using the [WAC](https://github.com/bytecodealliance/wac) language. It provides a framework for building CLI applications from modular WASM components.

## Installation

```bash
cargo install wacli
```

Or build from source:

```bash
git clone https://github.com/RAKUDEJI/wacli.git
cd wacli
cargo build --release
```

## Usage

### Initialize a new project

```bash
wacli init my-cli --name "example:my-cli"
```

### Build from manifest

```bash
wacli build -m wacli.json
```

### Compose components directly

```bash
wacli compose app.wac -o app.wasm -d "pkg:name=path.wasm"
```

### Plug components together

```bash
wacli plug socket.wasm --plug a.wasm --plug b.wasm -o out.wasm
```

### Check component imports

```bash
wacli check component.wasm -m wacli.json
```

## Manifest Format (wacli.json)

```json
{
  "package": {
    "name": "example:my-cli",
    "version": "0.1.0"
  },
  "framework": {
    "host": "components/host.component.wasm",
    "core": "components/core.component.wasm",
    "registry": "registry/registry.component.wasm"
  },
  "command": [
    {
      "name": "greet",
      "package": "example:greeter",
      "plugin": "plugins/greeter/greeter.component.wasm"
    }
  ],
  "output": {
    "path": "dist/my-cli.component.wasm"
  },
  "allowlist": [
    "wasi:filesystem/types",
    "wasi:cli/stdin"
  ]
}
```

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Final CLI (my-cli.component.wasm)        │
│  ┌─────────┐   ┌─────────┐   ┌──────────┐   ┌─────────┐    │
│  │  host   │──▶│ plugin  │──▶│ registry │──▶│  core   │    │
│  └─────────┘   └─────────┘   └──────────┘   └─────────┘    │
│   WASI bridge   Plugins      Command mgmt   Router         │
└─────────────────────────────────────────────────────────────┘
```

### Components

- **host**: Bridges WASI interfaces to `wacli:cli/host`
- **core**: Routes commands and exports `wasi:cli/run`
- **registry**: Manages command registration
- **plugins**: Implement commands via `wacli:cli/command`

## Framework Components

Pre-built framework components are available as release artifacts:

- `host.component.wasm` - WASI to wacli bridge
- `core.component.wasm` - Command router

Download from [Releases](https://github.com/RAKUDEJI/wacli/releases).

## WIT Interfaces

| Interface | Description |
|-----------|-------------|
| `wacli:cli/types` | Shared types (`exit-code`, `command-meta`, `command-error`) |
| `wacli:cli/host` | Host API for plugins (`args`, `stdout-write`, `exit`, etc.) |
| `wacli:cli/command` | Plugin export interface (`meta`, `run`) |
| `wacli:cli/registry` | Command management (`list-commands`, `run`) |

## License

Apache-2.0
