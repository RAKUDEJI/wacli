# wacli プロジェクト

WebAssembly Component Model ベースの CLI フレームワーク。

## アーキテクチャ

```
┌─────────────────────────────────────────────────────────────┐
│                    最終 CLI (my-cli.component.wasm)          │
│  ┌─────────┐   ┌─────────┐   ┌──────────┐   ┌─────────┐    │
│  │  host   │──▶│ plugin  │──▶│ registry │──▶│  core   │    │
│  └─────────┘   └─────────┘   └──────────┘   └─────────┘    │
│   WASI変換      プラグイン     コマンド管理    ルーター       │
└─────────────────────────────────────────────────────────────┘
```

## ディレクトリ構成

```
wacli/
├── Cargo.toml                  # Workspace root
├── crates/
│   ├── cli/                    # wacli CLIツール (Rust)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs         # CLIエントリポイント
│   │       ├── component_scan.rs   # コンポーネントスキャン
│   │       ├── registry_gen_wat.rs # Registry自動生成（WAT）
│   │       └── wac_gen.rs      # WAC生成
│   ├── plugin-loader/          # ランタイム用プラグインローダー
│   │   ├── Cargo.toml
│   │   └── src/lib.rs
│   └── wacli-cdk/              # プラグイン開発キット (crates.io公開)
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs          # CDKメインAPI
│           └── bindings.rs     # WIT生成コード
├── wit/                        # WIT定義
│   ├── cli/                    # wacli:cli パッケージ
│   │   ├── types.wit           # 共通型定義
│   │   ├── host-env.wit        # wacli/host-env インターフェース
│   │   ├── host-io.wit         # wacli/host-io インターフェース
│   │   ├── host-fs.wit         # wacli/host-fs インターフェース
│   │   ├── host-process.wit    # wacli/host-process インターフェース
│   │   ├── host-pipes.wit      # wacli/host-pipes インターフェース
│   │   ├── command.wit         # plugin world
│   │   ├── pipe.wit            # pipe-plugin world
│   │   ├── registry.wit        # registry インターフェース
│   │   ├── wasi-deps.wit       # WASI依存定義 (0.2.9)
│   │   └── wacli.wit           # worlds 定義
│   └── runner/                 # wacli:runner パッケージ
│       └── wacli-runner.wit    # 最終成果物のWIT定義
├── components/                 # フレームワークコンポーネント (Rust)
│   ├── host/                   # WASI → wacli/host-* ブリッジ
│   │   ├── Cargo.toml
│   │   └── src/lib.rs
│   └── core/                   # コマンドルーター
│       ├── Cargo.toml
│       └── src/lib.rs
└── CLAUDE.md
```

## wacli ツール

Rust製の単一バイナリCLI。外部ツール（wac, wasm-tools, jq）不要。

### インストール

```bash
cargo build --release -p wacli
# -> target/release/wacli
```

### コマンド

```bash
# プロジェクト初期化
wacli init [DIR]

# ディレクトリベースでビルド
wacli build --name "example:my-cli" [-o output.wasm]

# WAC直接合成
wacli compose app.wac -o app.wasm -d "pkg:name=path.wasm"

# プラグ合成
wacli plug socket.wasm --plug a.wasm --plug b.wasm -o out.wasm

# ランタイム実行
wacli run <component.wasm> [args...]

# Molt WASM-aware registry helper (/wasm/v1)
export MOLT_REGISTRY="https://registry.example.com"
# .env があれば自動で読み込み（開発用途）
export USERNAME="..."   # optional (Basic auth)
export PASSWORD="..."   # optional (Basic auth)
export MOLT_AUTH_HEADER="Authorization: Bearer $TOKEN"   # optional (explicit header)

# Framework components (host/core) can be pulled from the registry on init/build.
# Defaults:
#   WACLI_HOST_REPO=wacli/host, WACLI_CORE_REPO=wacli/core
#   WACLI_HOST_REFERENCE=v<cli-version>, WACLI_CORE_REFERENCE=v<cli-version>

wacli wasm wit <repo> <tag-or-digest>
wacli wasm interfaces <repo> <tag-or-digest>
wacli wasm dependencies <repo> <tag-or-digest>
wacli wasm search --export "<iface>" --import "<iface>" [--os wasip2]
```

### ビルドオプション

| オプション | デフォルト | 説明 |
|-----------|-----------|------|
| `--manifest` | (自動検出) | `wacli.json` マニフェスト（指定しない場合は `./wacli.json` があれば使用） |
| `--name` | "example:my-cli" | パッケージ名 |
| `--version` | "0.1.0" | パッケージバージョン |
| `-o, --output` | "my-cli.component.wasm" | 出力ファイルパス |
| `--defaults-dir` | "defaults" | フレームワークコンポーネントのディレクトリ |
| `--commands-dir` | "commands" | コマンドプラグインのディレクトリ |
| `--no-validate` | false | 検証をスキップ |
| `--print-wac` | false | 生成されたWACを表示（合成しない） |
| `--use-prebuilt-registry` | false | `defaults/registry.component.wasm` を使用（レジストリを自動生成しない） |

### ディレクトリ構成（ビルド時）

```
my-project/
├── wacli.json                  # ビルド用マニフェスト（`wacli init` が生成）
├── defaults/
│   ├── host.component.wasm       # 必須: WASIブリッジ
│   ├── core.component.wasm       # 必須: コマンドルーター
│   └── registry.component.wasm   # オプション: --use-prebuilt-registry 時のみ使用
├── .wacli/
│   └── registry.component.wasm   # 自動生成キャッシュ（編集しない）
└── commands/
    ├── greet.component.wasm      # コマンドプラグイン
    └── hello.component.wasm
```

`wacli build` の動作:
1. `defaults/` からフレームワークコンポーネント（host, core）を読み込み
2. `commands/` から `*.component.wasm` をスキャン
3. `wacli.json` の `build.commands` が設定されていて `MOLT_REGISTRY` があれば、OCIレジストリからコマンドコンポーネントを pull して `.wacli/commands/` にキャッシュ（`WACLI_REGISTRY_REFRESH=1` で再pull）
4. レジストリコンポーネントを毎回 `.wacli/registry.component.wasm` に生成（`--use-prebuilt-registry` の場合は `defaults/registry.component.wasm` を使用）
5. WAC言語で合成し、最終CLIを出力

## WIT インターフェース

### WASI バージョン
WASI 0.2.9 を使用。WASI は host/core 側で利用し、プラグインは直接インポートしない。

### wacli/types
共有型定義: `exit-code`, `command-meta`, `command-error`, `command-result`

### wacli/host-*
プラグイン向けホストAPIを機能別に分割:
- `wacli/host-env` (`args`, `env`)
- `wacli/host-io` (`stdout-write`, `stderr-write`, flush)
- `wacli/host-fs` (ファイルI/O)
- `wacli/host-process` (`exit`)
- `wacli/host-pipes` (パイプローダー)

### wacli/command
プラグインがエクスポート: `meta() -> command-meta`, `run(argv) -> command-result`

### wacli/registry
コマンド管理: `list-commands() -> list<command-meta>`, `run(name, argv) -> command-result`

### World定義

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

## コンポーネントのビルド

フレームワークコンポーネント（host, core）はRustで実装:

```bash
# ビルドスクリプトを使用
./scripts/build_components.sh

# または手動で
cargo build -p wacli-host --target wasm32-unknown-unknown --release
cargo build -p wacli-core --target wasm32-unknown-unknown --release

# WIT埋め込み + コンポーネント化
wasm-tools component embed wit/cli \
  target/wasm32-unknown-unknown/release/wacli_host.wasm \
  -o components/host/host.wasm --encoding utf8
wasm-tools component new components/host/host.wasm \
  -o components/host.component.wasm

wasm-tools component embed wit/cli \
  target/wasm32-unknown-unknown/release/wacli_core.wasm \
  -o components/core/core.wasm --encoding utf8
wasm-tools component new components/core/core.wasm \
  -o components/core.component.wasm
```

## wacli-cdk

プラグイン開発キット。crates.ioで公開。

### 特徴
- `Command` trait + `export!` マクロ
- `host` モジュール（host-* の集約: stdout/stderr, args/env, ファイルI/O, exit, pipes）
- `args` モジュール（引数パース）
- `io` モジュール（print, println, eprint, eprintln）

### プラグイン例

```rust
use wacli_cdk::{Command, CommandMeta, CommandResult, meta};

struct Show;

impl Command for Show {
    fn meta() -> CommandMeta {
        meta("show")
            .summary("Display file contents")
            .usage("show <FILE>")
            .build()
    }

    fn run(argv: Vec<String>) -> CommandResult {
        let name = argv.first().map(|s| s.as_str()).unwrap_or("World");
        wacli_cdk::io::println(format!("Hello, {name}!"));
        Ok(0)
    }
}

wacli_cdk::export!(Show);
```

## 注意事項

### wasm-tools 文字列エンコーディング
Rustの文字列はUTF-8。WIT埋め込み時は `--encoding utf8` を使用すること。

### bindings.rs の管理
`crates/wacli-cdk/src/bindings.rs` は wit-bindgen で生成したコードをコミット。
WIT変更時は再生成が必要:

```bash
wit-bindgen rust wit/cli --world plugin --out-dir crates/wacli-cdk/src/
```
