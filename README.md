# wacli

WebAssembly Component composition CLI tool.

[![CI](https://github.com/RAKUDEJI/wacli/actions/workflows/ci.yml/badge.svg)](https://github.com/RAKUDEJI/wacli/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/wacli.svg)](https://crates.io/crates/wacli)
[![License](https://img.shields.io/crates/l/wacli.svg)](LICENSE)

## Overview

wacli is a CLI tool for composing WebAssembly Components using the [WAC](https://github.com/bytecodealliance/wac) language. It provides a framework for building CLI applications from modular WASM components.

**Key Features:**
- Build CLI apps from modular WASM components
- Auto-generates registry component from command plugins
- Single binary, no external dependencies (wac, wasm-tools, jq)

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

### Features

| Feature | Default | Description |
|---------|---------|-------------|
| `runtime` | ✓ | Enables `wacli run` command (requires wasmtime) |

To build without runtime support (smaller binary, faster compile):

```bash
cargo install wacli --no-default-features
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

**Note:** `wacli build` scans `commands/**/*.component.wasm` recursively.

### Run the composed CLI (native host)

```bash
wacli run my-cli.component.wasm <command> [args...]
wacli run --dir /path/to/data my-cli.component.wasm <command> [args...]
wacli run --dir /path/to/data::/data my-cli.component.wasm <command> [args...]
```

**Tip:** `--dir` can appear before or after the component path. Use `--` if you
need to pass `--dir` through to the command itself.

**Note:** Direct `wasmtime run` is not supported because the composed CLI imports
`wacli:cli/pipe-runtime@1.0.0`, which is provided by `wacli run`.

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
    *.wit                     # Installed by `wacli init` (types/host/command/pipe, etc.)
```

Runtime layout (for `wacli run`):
```
my-cli.component.wasm
plugins/
  show/
    format/
      table.component.wasm
```

The `wacli build` command:
1. Scans `defaults/` for framework components (host, core, registry)
2. Scans `commands/` for command plugins (`*.component.wasm`)
3. Auto-generates `registry.component.wasm` if not present
4. Composes all components into the final CLI

The `wacli run` command:
- Runs a composed CLI component
- Loads pipes from `./plugins/<command>/...` relative to the current working directory
- Preopens the current directory and any `--dir HOST[::GUEST]` entries

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

- **host**: Bridges WASI interfaces to `wacli:cli/host-*`
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

**Tip:** The command name is derived from the component filename (e.g. `greet.component.wasm`
becomes `greet`). Keep it in sync with `meta("greet")` to avoid confusion.

**Note:** `wacli` calls `meta()` when building the command registry and when listing commands.
Keep `meta()` side‑effect‑free and fast.

For pipe plugins (the `pipe-plugin` world), see the “Building a Pipe Plugin” section in
`crates/wacli-cdk/README.md`.

### Host Access

Plugins do not import WASI directly. All host interactions go through the
`wacli:cli/host-*` interfaces (`host-env`, `host-io`, `host-fs`, `host-process`, `host-pipes`).

## Framework Components

Pre-built framework components are available as release artifacts:

- `host.component.wasm` - WASI to wacli bridge
- `core.component.wasm` - Command router

Download from [Releases](https://github.com/RAKUDEJI/wacli/releases).

## WIT Interfaces

| Interface | Description |
|-----------|-------------|
| `wacli:cli/types` | Shared types (`exit-code`, `command-meta`, `command-error`) |
| `wacli:cli/host-env` | Host environment (`args`, `env`) |
| `wacli:cli/host-io` | Host I/O (`stdout-write`, `stderr-write`, flush) |
| `wacli:cli/host-fs` | Host filesystem (`read-file`, `write-file`, `list-dir`) |
| `wacli:cli/host-process` | Host process (`exit`) |
| `wacli:cli/host-pipes` | Pipe loader (`list-pipes`, `load-pipe`) |
| `wacli:cli/command` | Plugin export interface (`meta`, `run`) |
| `wacli:cli/registry` | Command management (`list-commands`, `run`) |
| `wacli:cli/pipe` | Pipe export interface (`meta`, `process`) |

### Plugin World

```wit
world plugin {
  import host-env;
  import host-io;
  import host-fs;
  import host-process;
  import host-pipes;
  export command;
}
```

**Note:** These unqualified imports are shorthand for the same-package interfaces.
When embedded into a component they resolve to fully-qualified names like
`wacli:cli/host-env@1.0.0`. This is expected and matches what `wacli` provides.

## License

Apache-2.0
