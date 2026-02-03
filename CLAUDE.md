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
│   └── wacli-cdk/              # プラグイン開発キット (crates.io公開)
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs          # CDKメインAPI
│           └── bindings.rs     # WIT生成コード
├── wit/                        # WIT定義
│   ├── wacli.wit               # マスターWIT定義 (WASI 0.2.9)
│   ├── registry.wit            # Registry用WIT（ベース）
│   └── wacli-runner.wit        # 最終成果物のWIT定義
├── components/                 # フレームワークコンポーネント (Rust)
│   ├── host/                   # WASI → wacli/host ブリッジ
│   │   ├── Cargo.toml
│   │   ├── src/lib.rs
│   │   └── wit/
│   └── core/                   # コマンドルーター
│       ├── Cargo.toml
│       ├── src/lib.rs
│       └── wit/
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
```

### ビルドオプション

| オプション | デフォルト | 説明 |
|-----------|-----------|------|
| `--name` | "example:my-cli" | パッケージ名 |
| `--version` | "0.1.0" | パッケージバージョン |
| `-o, --output` | "my-cli.component.wasm" | 出力ファイルパス |
| `--no-validate` | false | 検証をスキップ |
| `--print-wac` | false | 生成されたWACを表示（合成しない） |

### ディレクトリ構成（ビルド時）

```
my-project/
├── defaults/
│   ├── host.component.wasm       # 必須: WASIブリッジ
│   ├── core.component.wasm       # 必須: コマンドルーター
│   └── registry.component.wasm   # オプション: 未指定時は自動生成
└── commands/
    ├── greet.component.wasm      # コマンドプラグイン
    └── hello.component.wasm
```

`wacli build` の動作:
1. `defaults/` からフレームワークコンポーネント（host, core）を読み込み
2. `commands/` から `*.component.wasm` をスキャン
3. `registry.component.wasm` がなければWATテンプレートから自動生成
4. WAC言語で合成し、最終CLIを出力

## WIT インターフェース

### WASI バージョン
WASI 0.2.9 を使用。プラグインは `wasi-capabilities` を通じてファイルシステムとランダムにアクセス可能。

### wacli/types
共有型定義: `exit-code`, `command-meta`, `command-error`, `command-result`

### wacli/host
プラグイン向けホストAPI: `args`, `stdout-write`, `stderr-write`, `exit` など

### wacli/command
プラグインがエクスポート: `meta() -> command-meta`, `run(argv) -> command-result`

### wacli/registry
コマンド管理: `list-commands() -> list<command-meta>`, `run(name, argv) -> command-result`

### World定義

```wit
world plugin {
  import host;
  include wasi-capabilities;  // filesystem, random
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
wasm-tools component embed components/host/wit \
  target/wasm32-unknown-unknown/release/wacli_host.wasm \
  -o components/host/host.wasm --encoding utf8
wasm-tools component new components/host/host.wasm \
  -o components/host.component.wasm

wasm-tools component embed components/core/wit \
  target/wasm32-unknown-unknown/release/wacli_core.wasm \
  -o components/core/core.wasm --encoding utf8
wasm-tools component new components/core/core.wasm \
  -o components/core.component.wasm
```

## wacli-cdk

プラグイン開発キット。crates.ioで公開。

### 特徴
- `Command` trait + `export!` マクロ
- `wasi` モジュール再エクスポート（ファイルシステム、ランダム）
- `host` モジュール（stdout, stderr, args, env）
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
        use wacli_cdk::wasi::filesystem::preopens::get_directories;
        // WASI filesystem APIを使用可能
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
wit-bindgen rust crates/wacli-cdk/wit --world plugin --out-dir crates/wacli-cdk/src/
```
