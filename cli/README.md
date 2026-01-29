# fw-cli Framework

WebAssembly Component Model ベースの CLI フレームワーク。

## 構造

```
cli/
  wit/
    fw-cli.wit           # フレームワーク ABI（host/command/registry）
    fw-cli-runner.wit    # 最小 targets world（wasi:cli/run のみ）
  components/
    (fw-cli-host.wasm)   # WASI 境界コンポーネント（要ビルド）
    (fw-cli-core.wasm)   # ルータコンポーネント（要ビルド）
  tools/
    fw-cli-gen-registry  # registry.wit + registry.wasm 生成
    fw-cli-compose       # .wac 生成 + wac compose 実行
    fw-cli-check-imports # import 許容集合チェック
  allowlist/
    wasi-imports-*.txt   # 許容 WASI import 一覧
  example/
    tool.toml            # サンプルプロジェクト設定
    deps/                # 依存コンポーネント配置場所
    dist/                # ビルド成果物
```

## 事前準備（ツールのインストール）

```sh
# WAC（合成）
cargo install wac-cli

# wasm-tools（検査）
cargo install wasm-tools

# wasmtime（実行）
curl https://wasmtime.dev/install.sh -sSf | bash

# wkg（パッケージ取得・配布、任意）
cargo install wkg
```

## 許容 WASI Import

- `wasi:filesystem/types`, `wasi:filesystem/preopens`
- `wasi:random/random`, `wasi:random/insecure`, `wasi:random/insecure-seed`
- `wasi:cli/environment`, `wasi:cli/exit`
- `wasi:cli/stdin`, `wasi:cli/stdout`, `wasi:cli/stderr`
- `wasi:cli/terminal-input`, `wasi:cli/terminal-output`
- `wasi:cli/terminal-stdin`, `wasi:cli/terminal-stdout`, `wasi:cli/terminal-stderr`

---

## CLI製品側の使い方：`tool.wasm` を作って動かす

### 1. プラグイン集合を宣言

`tool.toml` を作成:

```toml
[tool]
package = "example:tool"
version = "0.1.0"

[framework]
fw_cli = "fw:cli@1.0.0"
wasi_snapshot = "0.3.0-rc-2026-01-06"

[[command]]
name = "fmt"
package = "acme:fmt@0.4.1"

[[command]]
name = "lint"
package = "acme:lint@0.2.0"
aliases = ["check"]
```

### 2. 依存コンポーネントを配置

`deps/` ディレクトリに配置（wac が自動で探す）:

```
deps/
  fw/
    cli-host.wasm
    cli-core.wasm
  acme/
    fmt.wasm
    lint.wasm
  example/
    tool-registry.wasm
```

または `--dep` で明示:

```sh
wac compose --dep acme:fmt=./path/to/fmt.wasm ...
```

### 3. registry を生成

```sh
fw-cli-gen-registry tool.toml \
  --out-wit dist/registry.wit \
  --out-wasm dist/registry.wasm
```

### 4. 合成レシピ `.wac` を生成

```sh
fw-cli-compose init tool.toml --out dist/tool.wac
```

### 5. WAC で合成して単体 `.wasm` を作る

```sh
fw-cli-compose build dist/tool.wac --out dist/tool.wasm
# または直接:
wac compose -o dist/tool.wasm dist/tool.wac
```

### 6. 許容 import だけか検査（必須）

```sh
fw-cli-check-imports dist/tool.wasm \
  --allowlist ../allowlist/wasi-imports-0.3.0-rc-2026-01-06.txt \
  --out dist/imports.json
```

### 7. wasmtime で実行

```sh
# ファイルシステムアクセスが必要な場合
wasmtime --dir . dist/tool.wasm fmt ./src

# 通常実行
wasmtime dist/tool.wasm lint --fix ./src
```

**Note**: wasmtime のオプションは Wasm ファイルより**前**、Wasm への引数は**後**に置く。

---

## プラグイン作者の使い方

### 1. `fw:cli/command` を実装

- `fw:cli/host` を import し、`fw:cli/command` を export
- `meta()` と `run(argv)` を実装

### 2. コンポーネントとしてビルド

```sh
cargo component build --release
```

### 3. 型検証（wac targets）

```sh
wac targets plugin.wasm wit/fw-cli.wit --world plugin
```

### 4. 配布（wkg を使う場合）

```sh
wkg publish acme:fmt@0.4.1 ./plugin.wasm
wkg get acme:fmt@0.4.1
```

---

## 日常運用コマンドまとめ

### 生成・合成・検査（ビルダー側）

```sh
# 1. registry 生成
fw-cli-gen-registry tool.toml --out-wit dist/registry.wit --out-wasm dist/registry.wasm

# 2. .wac テンプレート生成
fw-cli-compose init tool.toml --out dist/tool.wac

# 3. 最終 wasm 合成
fw-cli-compose build dist/tool.wac --out dist/tool.wasm

# 4. import 検査
fw-cli-check-imports dist/tool.wasm --allowlist allowlist/wasi-imports-*.txt --out dist/imports.json
```

### 実行（利用者側）

```sh
wasmtime --dir . dist/tool.wasm <subcommand> [args...]
```

---

## 依存ツール

- [wac](https://github.com/bytecodealliance/wac) - コンポーネント合成
- [wasm-tools](https://github.com/bytecodealliance/wasm-tools) - import 検査
- [wasmtime](https://wasmtime.dev/) - 実行
- [wkg](https://component-model.bytecodealliance.org/) - パッケージ配布（任意）
