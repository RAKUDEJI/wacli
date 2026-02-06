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
- Optional registry integration for framework components, plugins, and WIT/index queries (Molt spec)

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

`wacli` reads registry settings from environment variables. For local/dev usage
you can put them in a `.env` file (loaded automatically if present).

This repository includes a sample: `.env.example`.

### Initialize a new project

```bash
wacli init my-cli
```

Download framework components in one step:

```bash
wacli init my-cli --with-components
```

If `MOLT_REGISTRY` is set, `--with-components` pulls `host.component.wasm` and
`core.component.wasm` from the registry via `/v2` instead of GitHub Releases.
By default it uses:

- `WACLI_HOST_REPO` (default `wacli/host`) with `WACLI_HOST_REFERENCE` (default `v<cli-version>`)
- `WACLI_CORE_REPO` (default `wacli/core`) with `WACLI_CORE_REFERENCE` (default `v<cli-version>`)

This creates the directory structure:
```
my-cli/
  wacli.json
  defaults/
  commands/
  wit/
    command.wit
```

### Build from defaults/ and commands/

```bash
cd my-cli
wacli build
```

If `defaults/host.component.wasm` or `defaults/core.component.wasm` is missing
and `MOLT_REGISTRY` is set, `wacli build` will pull the missing framework
components from the registry into `.wacli/framework/` and use the cached files.

`wacli init` creates a `wacli.json` manifest so you don't need to repeat build flags.

Example `wacli.json`:
```json
{
  "schemaVersion": 1,
  "build": {
    "name": "example:my-cli",
    "version": "0.1.0",
    "output": "my-cli.component.wasm",
    "defaultsDir": "defaults",
    "commandsDir": "commands"
  }
}
```

Optional: resolve command plugins from an OCI registry (instead of requiring
local `commands/*.component.wasm` files):

```json
{
  "schemaVersion": 1,
  "build": {
    "name": "example:my-cli",
    "version": "0.1.0",
    "output": "my-cli.component.wasm",
    "defaultsDir": "defaults",
    "commandsDir": "commands",
    "commands": [
      { "name": "greet", "repo": "example/greet", "reference": "1.0.0" }
    ]
  }
}
```

This requires `MOLT_REGISTRY` and auth via either `MOLT_AUTH_HEADER` or
`USERNAME`/`PASSWORD` (Basic). Pulled
components are cached under `.wacli/commands/`. Set `WACLI_REGISTRY_REFRESH=1`
to force re-pull.

Options:
- `--manifest`: Path to a wacli manifest (defaults to `./wacli.json` if present)
- `--name`: Package name (default: "example:my-cli")
- `--version`: Package version (default: "0.1.0")
- Package name and version are combined as `name@version` unless `name` already contains `@`.
- `-o, --output`: Output file path (default: "my-cli.component.wasm")
- `--defaults-dir`: Defaults directory (default: "defaults")
- `--commands-dir`: Commands directory (default: "commands")
- `--no-validate`: Skip validation of the composed component
- `--print-wac`: Print generated WAC without composing
- `--use-prebuilt-registry`: Use `defaults/registry.component.wasm` instead of generating a registry

**Note:** `wacli build` scans `commands/**/*.component.wasm` recursively, and
also resolves any registry plugins configured in `build.commands`.

### Run the composed CLI (native host)

```bash
wacli run my-cli.component.wasm <command> [args...]
wacli run --dir /path/to/data my-cli.component.wasm <command> [args...]
wacli run --dir /path/to/data::/data my-cli.component.wasm <command> [args...]
```

**Tip:** `--dir` can appear before or after the component path. Use `--` if you
need to pass `--dir` through to the command itself.

**Note:** Direct `wasmtime run` is not supported because the composed CLI imports
`wacli:cli/pipe-runtime@2.0.0`, which is provided by `wacli run`.

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

### Molt WASM-aware registry helper commands (/wasm/v1)

These commands call the registry's `/wasm/v1` endpoints to fetch WIT and query
the WASM index.

Set `MOLT_REGISTRY` or pass `--registry` on each command:

```bash
export MOLT_REGISTRY="https://registry.example.com"

# Optional auth
# export USERNAME="..."
# export PASSWORD="..."
# export MOLT_AUTH_HEADER="Authorization: Bearer $TOKEN"
# wacli wasm wit ... --header "Authorization: Bearer $TOKEN"   # per-command override
```

Fetch WIT for a repo + tag (prints WIT text to stdout):

```bash
wacli wasm wit example/repo 1.0.0 > component.wit
```

By default `wacli wasm wit` uses `--artifact-type application/vnd.wasm.wit.v1+text`.

Fetch indexed imports/exports:

```bash
wacli wasm interfaces example/repo 1.0.0
```

Search by imports/exports (AND semantics):

```bash
wacli wasm search --export "wacli:cli/command@2.0.0" --os wasip2
```

## Project Structure

```
my-cli/
  wacli.json                  # Build manifest (created by `wacli init`)
  defaults/
    host.component.wasm       # Required: WASI to wacli bridge
    core.component.wasm       # Required: Command router
    registry.component.wasm   # Optional: Used only with --use-prebuilt-registry
  commands/
    greet.component.wasm      # Command plugins (*.component.wasm)
    show.component.wasm
  .wacli/
    registry.component.wasm   # Auto-generated build cache (do not edit)
    framework/                # Cached host/core pulls (optional)
    commands/                 # Cached registry plugin pulls (optional)
  wit/
    *.wit                     # Installed by `wacli init` (types/host/command/pipe, etc.)
```

**Note:** `.wacli/` contains build cache artifacts. It's safe to add it to `.gitignore`.

Runtime layout (for `wacli run`):
```
my-cli.component.wasm
plugins/
  show/
    format/
      table.component.wasm
```

The `wacli build` command:
1. Scans `defaults/` for framework components (host, core)
2. If host/core is missing and `MOLT_REGISTRY` is set, pulls them into `.wacli/framework/`
3. Scans `commands/` for command plugins (`*.component.wasm`)
4. If `build.commands` is set, pulls those plugin components into `.wacli/commands/`
5. Generates a registry component into `.wacli/registry.component.wasm` (or uses `defaults/registry.component.wasm` with `--use-prebuilt-registry`)
6. Composes all components into the final CLI

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

Download from [Releases](https://github.com/RAKUDEJI/wacli/releases), or configure
`MOLT_REGISTRY` and use `wacli init --with-components` / `wacli build` to pull
them from an OCI registry.

## WIT Interfaces

| Interface | Description |
|-----------|-------------|
| `wacli:cli/types` | Shared types (`exit-code`, `command-meta`, `command-error`) |
| `wacli:cli/host-env` | Host environment (`args`, `env`) |
| `wacli:cli/host-io` | Host I/O (`stdout-write`, `stderr-write`, flush) |
| `wacli:cli/host-fs` | Host filesystem (`read-file`, `write-file`, `create-dir`, `list-dir`) |
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
`wacli:cli/host-env@2.0.0`. This is expected and matches what `wacli` provides.

## License

Apache-2.0
