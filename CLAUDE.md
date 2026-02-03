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
│   └── cli/                    # wacli CLIツール (Rust)
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs         # CLIエントリポイント
│           ├── component_scan.rs   # コンポーネントスキャン
│           ├── registry_gen_wat.rs # Registry自動生成（WAT）
│           ├── registry_template.wat # WATテンプレート
│           └── wac_gen.rs      # WAC生成
├── wit/                        # WIT定義
│   ├── wacli.wit               # マスターWIT定義
│   ├── registry.wit            # Registry用WIT（ベース）
│   └── wacli-runner.wit        # 最終成果物のWIT定義
├── components/                 # フレームワークコンポーネント
│   ├── host/                   # WASI → wacli/host ブリッジ
│   └── core/                   # コマンドルーター
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

### wacli/types
共有型定義: `exit-code`, `command-meta`, `command-error`, `command-result`

### wacli/host
プラグイン向けホストAPI: `args`, `stdout-write`, `stderr-write`, `exit` など

### wacli/command
プラグインがエクスポート: `meta() -> command-meta`, `run(argv) -> command-result`

### wacli/registry
コマンド管理: `list-commands() -> list<command-meta>`, `run(name, argv) -> command-result`

## コンポーネントのビルド

各コンポーネントで以下を実行:

```bash
# MoonBitビルド
cd <component>/gen && moon build --target wasm

# WIT埋め込み + コンポーネント化
cd <component>
wasm-tools component embed wit gen/_build/wasm/release/build/gen/gen.wasm -o <name>.wasm --encoding utf16
wasm-tools component new <name>.wasm -o <name>.component.wasm
```

## 注意事項

### wit-bindgen 実行時
`wit-bindgen moonbit wit --out-dir gen` を実行すると stub.mbt が上書きされる。
実装済みの stub.mbt は事前にバックアップするか、git で管理すること。

### MoonBit文字列エンコーディング
MoonBitの文字列はUTF-16。WASM出力には `--encoding utf16` が必要。
stdout出力には `@encoding/utf8.encode()` で変換すること。
