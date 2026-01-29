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
├── plugins/greeter/            # greetプラグイン
│   ├── wit/world.wit
│   └── gen/
├── registry/                   # greeterを登録するレジストリ
│   ├── wit/world.wit
│   └── gen/
├── compose.wac                 # WAC合成ファイル
└── hello-cli.component.wasm    # 最終成果物
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

### 2. WAC合成

```bash
cd examples/hello
wac compose compose.wac \
  -d "fw:cli-host=../../cli/components/host/host.component.wasm" \
  -d "fw:cli-core=../../cli/components/core/core.component.wasm" \
  -d "example:greeter=plugins/greeter/greeter.component.wasm" \
  -d "example:hello-registry=registry/registry.component.wasm" \
  -o hello-cli.component.wasm
```

### 3. 実行

```bash
wasmtime run hello-cli.component.wasm greet Claude
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
# WIT確認
wasm-tools component wit <file>.component.wasm

# コンポーネント検証
wasm-tools validate <file>.component.wasm

# WAC構文確認
wac parse compose.wac
```
