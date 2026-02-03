# wacli

WebAssembly Component composition CLI tool.

[![CI](https://github.com/aspect-build/wacli/actions/workflows/ci.yml/badge.svg)](https://github.com/aspect-build/wacli/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/wacli.svg)](https://crates.io/crates/wacli)
[![License](https://img.shields.io/crates/l/wacli.svg)](LICENSE)

## Overview

wacli is a CLI tool for composing WebAssembly Components using the [WAC](https://github.com/bytecodealliance/wac) language. It provides a framework for building CLI applications from modular WASM components.

**Key Features:**
- Build CLI apps from modular WASM components
- Plugins have access to WASI filesystem and random APIs
- Auto-generates registry component from command plugins
- Single binary, no external dependencies (wac, wasm-tools, jq)

## Installation

```bash
cargo install wacli
```

Or build from source:

```bash
git clone https://github.com/aspect-build/wacli.git
cd wacli
cargo build --release
```

## Usage

### Initialize a new project

```bash
wacli init my-cli
```

Download framework components in one step:

```bash
wacli init my-cli --with-components
```

This creates the directory structure:
```
my-cli/
  defaults/
  commands/
  wit/
    command.wit
```

### Build from defaults/ and commands/

```bash
cd my-cli
wacli build --name "example:my-cli"
```

Options:
- `--name`: Package name (default: "example:my-cli")
- `--version`: Package version (default: "0.1.0")
- `-o, --output`: Output file path (default: "my-cli.component.wasm")
- `--no-validate`: Skip validation of the composed component
- `--print-wac`: Print generated WAC without composing

### Compose components directly

```bash
wacli compose app.wac -o app.wasm -d "pkg:name=path.wasm"
```

### Plug components together

```bash
wacli plug socket.wasm --plug a.wasm --plug b.wasm -o out.wasm
```

### Self update

```bash
wacli self-update
```

## Project Structure

```
my-cli/
  defaults/
    host.component.wasm       # Required: WASI to wacli bridge
    core.component.wasm       # Required: Command router
    registry.component.wasm   # Optional: Auto-generated if missing
  commands/
    greet.component.wasm      # Command plugins (*.component.wasm)
    show.component.wasm
  wit/
    command.wit               # Plugin interface for components
```

The `wacli build` command:
1. Scans `defaults/` for framework components (host, core, registry)
2. Scans `commands/` for command plugins (`*.component.wasm`)
3. Auto-generates `registry.component.wasm` if not present
4. Composes all components into the final CLI

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

## Plugin Development

Plugins are built using [wacli-cdk](https://crates.io/crates/wacli-cdk):

```rust
use wacli_cdk::{Command, CommandMeta, CommandResult, meta};

struct Greet;

impl Command for Greet {
    fn meta() -> CommandMeta {
        meta("greet")
            .summary("Greet someone")
            .usage("greet [NAME]")
            .build()
    }

    fn run(argv: Vec<String>) -> CommandResult {
        let name = argv.first().map(|s| s.as_str()).unwrap_or("World");
        wacli_cdk::io::println(format!("Hello, {name}!"));
        Ok(0)
    }
}

wacli_cdk::export!(Greet);
```

### WASI Capabilities

Plugins have access to WASI 0.2.9 capabilities:

- **Filesystem**: `wacli_cdk::wasi::filesystem::{types, preopens}`
- **Random**: `wacli_cdk::wasi::random::{random, insecure, insecure_seed}`

```rust
fn run(argv: Vec<String>) -> CommandResult {
    use wacli_cdk::wasi::filesystem::preopens::get_directories;

    // Access filesystem via WASI preopens
    let dirs = get_directories();
    // ...
    Ok(0)
}
```

## Framework Components

Pre-built framework components are available as release artifacts:

- `host.component.wasm` - WASI to wacli bridge
- `core.component.wasm` - Command router

Download from [Releases](https://github.com/aspect-build/wacli/releases).

## WIT Interfaces

| Interface | Description |
|-----------|-------------|
| `wacli:cli/types` | Shared types (`exit-code`, `command-meta`, `command-error`) |
| `wacli:cli/host` | Host API for plugins (`args`, `stdout-write`, `exit`, etc.) |
| `wacli:cli/command` | Plugin export interface (`meta`, `run`) |
| `wacli:cli/registry` | Command management (`list-commands`, `run`) |

### Plugin World

```wit
world plugin {
  import host;
  include wasi-capabilities;  // filesystem, random
  export command;
}
```

## License

Apache-2.0
