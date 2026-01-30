# fw-cli プロジェクト

WebAssembly Component Model ベースの CLI フレームワーク。MoonBit で実装。

## アーキテクチャ

```
┌─────────────────────────────────────────────────────────────┐
│                    最終 CLI (hello-cli.component.wasm)       │
│  ┌─────────┐   ┌─────────┐   ┌──────────┐   ┌─────────┐    │
│  │  host   │──▶│ greeter │──▶│ registry │──▶│  core   │    │
│  └─────────┘   └─────────┘   └──────────┘   └─────────┘    │
│   WASI変換      プラグイン     コマンド管理    ルーター       │
└─────────────────────────────────────────────────────────────┘
```

## ディレクトリ構成

```
tools/
└── wacli/                      # WAC合成CLIツール (Rust)
    ├── Cargo.toml
    └── src/

cli/
├── wit/fw-cli.wit              # マスターWIT定義（参照用）
├── components/
│   ├── host/                   # WASI → fw:cli/host ブリッジ
│   │   ├── wit/world.wit       # host-provider world定義
│   │   └── gen/                # wit-bindgen生成 + 実装
│   └── core/                   # コマンドルーター
│       ├── wit/world.wit       # core world定義
│       └── gen/                # wit-bindgen生成 + 実装

examples/hello/
├── wacli.toml                  # マニフェスト（wacli build用）
├── plugins/greeter/            # greetプラグイン
│   ├── wit/world.wit
│   └── gen/
├── registry/                   # greeterを登録するレジストリ
│   ├── wit/world.wit
│   └── gen/
├── compose.wac                 # WAC合成ファイル（直接合成用）
└── dist/
    └── hello-cli.component.wasm  # 最終成果物
```

## wacli ツール

Rust製の単一バイナリCLI。外部ツール（wac, wasm-tools, jq）不要。

### インストール

```bash
cd tools/wacli && cargo build --release
# -> target/release/wacli
```

### コマンド

```bash
# プロジェクト初期化
wacli init [DIR] --name "example:my-cli"

# マニフェストからビルド
wacli build -m wacli.toml [-o output.wasm]

# WAC直接合成
wacli compose app.wac -o app.wasm -d "pkg:name=path.wasm"

# プラグ合成
wacli plug socket.wasm --plug a.wasm --plug b.wasm -o out.wasm

# import検査
wacli check component.wasm --allowlist allowed.txt [--json]
```

### wacli.toml 形式

```toml
[package]
name = "example:hello-cli"
version = "0.1.0"

[framework]
host = "../../cli/components/host/host.component.wasm"
core = "../../cli/components/core/core.component.wasm"
registry = "registry/registry.component.wasm"

[[command]]
name = "greet"
package = "example:greeter"
plugin = "plugins/greeter/greeter.component.wasm"

[output]
path = "dist/hello-cli.component.wasm"
```

## ビルド手順

### 1. コンポーネントのビルド

各コンポーネントで以下を実行:

```bash
# MoonBitビルド
cd <component>/gen && moon build --target wasm

# WIT埋め込み + コンポーネント化
cd <component>
wasm-tools component embed wit gen/_build/wasm/release/build/gen/gen.wasm -o <name>.wasm --encoding utf16
wasm-tools component new <name>.wasm -o <name>.component.wasm
```

### 2. WAC合成（wacli使用）

```bash
cd examples/hello

# マニフェストからビルド（推奨）
wacli build -m wacli.toml

# または直接WAC合成
wacli compose compose.wac \
  -d "fw:cli-host=../../cli/components/host/host.component.wasm" \
  -d "fw:cli-core=../../cli/components/core/core.component.wasm" \
  -d "example:greeter=plugins/greeter/greeter.component.wasm" \
  -d "example:hello-registry=registry/registry.component.wasm" \
  -o hello-cli.component.wasm
```

### 3. 実行

```bash
wasmtime run dist/hello-cli.component.wasm greet Claude
# => Hello, Claude!
```

## WIT インターフェース

### fw:cli/types
共有型定義: `exit-code`, `command-meta`, `command-error`, `command-result`

### fw:cli/host
プラグイン向けホストAPI: `args`, `stdout-write`, `stderr-write`, `exit` など

### fw:cli/command
プラグインがエクスポート: `meta() -> command-meta`, `run(argv) -> command-result`

### fw:cli/registry
コマンド管理: `list-commands() -> list<command-meta>`, `run(name, argv) -> command-result`

## 注意事項

### wit-bindgen 実行時
`wit-bindgen moonbit wit --out-dir gen` を実行すると stub.mbt が上書きされる。
実装済みの stub.mbt は事前にバックアップするか、git で管理すること。

### 手動編集が必要なファイル
- `gen/gen/interface/.../stub.mbt` - インターフェース実装
- `gen/gen/interface/.../moon.pkg.json` - import追加が必要な場合

### MoonBit文字列エンコーディング
MoonBitの文字列はUTF-16。WASM出力には `--encoding utf16` が必要。
stdout出力には `@encoding/utf8.encode()` で変換すること。

## コマンド一覧

```bash
# wacli（推奨）
wacli build -m wacli.toml          # マニフェストからビルド
wacli compose app.wac -o out.wasm  # WAC直接合成
wacli check comp.wasm --allowlist allow.txt  # import検査

# WIT確認
wasm-tools component wit <file>.component.wasm

# コンポーネント検証
wasm-tools validate <file>.component.wasm
```
