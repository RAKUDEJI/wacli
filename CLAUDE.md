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
├── Cargo.toml                  # wacli CLIツール (Rust)
├── src/
├── cli/                        # フレームワークコンポーネント
│   ├── wit/wacli.wit           # マスターWIT定義
│   └── components/
│       ├── host/               # WASI → wacli/host ブリッジ
│       └── core/               # コマンドルーター
├── CLAUDE.md
└── README.md
```

## wacli ツール

Rust製の単一バイナリCLI。外部ツール（wac, wasm-tools, jq）不要。

### インストール

```bash
cargo build --release
# -> target/release/wacli
```

### コマンド

```bash
# プロジェクト初期化
wacli init [DIR] --name "example:my-cli"

# マニフェストからビルド
wacli build -m wacli.json [-o output.wasm]

# WAC直接合成
wacli compose app.wac -o app.wasm -d "pkg:name=path.wasm"

# プラグ合成
wacli plug socket.wasm --plug a.wasm --plug b.wasm -o out.wasm

# import検査
wacli check component.wasm --allowlist allowed.txt [--json]
```

### wacli.json 形式

```json
{
  "package": {
    "name": "example:my-cli",
    "version": "0.1.0"
  },
  "framework": {
    "host": "path/to/host.component.wasm",
    "core": "path/to/core.component.wasm",
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
  }
}
```

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
