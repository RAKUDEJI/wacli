# wacli Framework

WebAssembly Component Model ベースの CLI フレームワーク。

## 構造

```
cli/
  wit/
    wacli.wit            # フレームワーク ABI（host/command/registry）
    wacli-runner.wit     # 最小 targets world（wasi:cli/run のみ）
  components/
    host/                # WASI 境界コンポーネント
    core/                # ルータコンポーネント
  allowlist/
    wasi-imports-*.txt   # 許容 WASI import 一覧（wacli.json に転記して使う）
```

## wacli CLIツール

Rust製の単一バイナリCLI。外部ツール（wac, wasm-tools, jq）不要。

### コマンド

```sh
# プロジェクト初期化
wacli init [DIR] --name "example:my-cli"

# マニフェストからビルド
wacli build -m wacli.json [-o output.wasm]

# WAC直接合成
wacli compose app.wac -o app.wasm -d "pkg:name=path.wasm"

# プラグ合成
wacli plug socket.wasm --plug a.wasm --plug b.wasm -o out.wasm

# import検査
wacli check component.wasm -m wacli.json [--json]
```

## 許容 WASI Import

- `wasi:filesystem/types`, `wasi:filesystem/preopens`
- `wasi:random/random`, `wasi:random/insecure`, `wasi:random/insecure-seed`
- `wasi:cli/environment`, `wasi:cli/exit`
- `wasi:cli/stdin`, `wasi:cli/stdout`, `wasi:cli/stderr`
- `wasi:cli/terminal-input`, `wasi:cli/terminal-output`
- `wasi:cli/terminal-stdin`, `wasi:cli/terminal-stdout`, `wasi:cli/terminal-stderr`

---

## CLI製品側の使い方

### 1. wacli.json を作成

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
  },
  "allowlist": [
    "wasi:filesystem/types",
    "wasi:cli/stdin"
  ]
}
```

### 2. ビルド

```sh
wacli build -m wacli.json
```

### 3. 許容 import だけか検査

```sh
wacli check dist/my-cli.component.wasm \
  -m wacli.json
```

### 4. wasmtime で実行

```sh
wasmtime dist/my-cli.component.wasm greet Claude
```

---

## プラグイン作者の使い方

### 1. `wacli/command` を実装

- `wacli/host` を import し、`wacli/command` を export
- `meta()` と `run(argv)` を実装

### 2. コンポーネントとしてビルド

```sh
cargo component build --release
```

### 3. 型検証（wac targets）

```sh
wac targets plugin.wasm cli/wit/wacli.wit --world plugin
```

---

## 依存ツール

- [wasmtime](https://wasmtime.dev/) - 実行
- [wkg](https://component-model.bytecodealliance.org/) - パッケージ配布（任意）
