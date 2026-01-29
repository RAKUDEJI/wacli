# wacli Phase 2 計画: manifest駆動ビルド

## 設計方針

**案C採用**: ルーターを合成時に生成する（slot/テンプレではなく）

- registryコンポーネントを manifest から自動生成
- ローカルパス解決のみ（レジストリ統合は後回し）

## 依存関係

```
外部コマンド:
- moon (MoonBit コンパイラ) - ユーザーがプラグイン/registry書くため

wacli内蔵 (crate):
- wit-parser = "0.244.0"     # WIT解析
- wit-component = "0.244.0"  # module → component 変換
- wac-parser                 # WAC合成
```

**wasm-tools CLI への依存なし** - wit-component crate で内部処理

---

## Phase 2 サブフェーズ

### Phase 2a: 基本manifest + WAC生成

**目標**: `wacli.toml` から WAC を自動生成して合成

```
wacli.toml → WAC生成 → wac-parser合成 → tool.wasm
```

**前提**: 全コンポーネントは事前にビルド済み (.component.wasm)

#### wacli.toml 形式

```toml
[package]
name = "example:hello-cli"
version = "0.1.0"

[framework]
host = "../../cli/components/host/host.component.wasm"
core = "../../cli/components/core/core.component.wasm"

# 事前ビルド済みのregistryを指定（2aの制限）
registry = "registry/registry.component.wasm"

[[command]]
name = "greet"
plugin = "plugins/greeter/greeter.component.wasm"

[output]
path = "dist/hello-cli.component.wasm"
```

#### 生成されるWAC（内部）

```wac
package example:hello-cli;

let host = new fw:cli-host { ... };

let greet = new example:greeter {
  types: host.types,
  host: host.host
};

let registry = new example:hello-registry {
  types: host.types,
  greet: greet.command
};

let core = new fw:cli-core {
  types: host.types,
  host: host.host,
  registry: registry.registry
};

export core.run;
```

#### 実装タスク

1. `wacli.toml` パーサー（toml crate）
2. WAC文字列生成ロジック
3. `wacli build` コマンド追加
4. `wacli init` コマンド（雛形生成）

---

### Phase 2b: Registry自動生成 + Component化

**目標**: manifest の `[[command]]` からregistryコンポーネントを自動生成

```
wacli.toml → MoonBit生成 → moon build → wit-component → registry.component.wasm → WAC合成
```

#### 生成するもの

1. `.wacli/registry/wit/world.wit` - コマンドに応じたWIT
2. `.wacli/registry/gen/.../stub.mbt` - ディスパッチロジック
3. `.wacli/registry/gen/moon.pkg.json` - import設定

#### wacli内部でのcomponent化

```rust
use wit_component::ComponentEncoder;
use wit_parser::UnresolvedPackageGroup;

fn module_to_component(
    module_bytes: &[u8],
    wit_path: &Path,
    world: &str,
) -> Result<Vec<u8>> {
    // WIT解析
    let pkg = UnresolvedPackageGroup::parse_dir(wit_path)?;

    // Component化 (wasm-tools component new 相当)
    ComponentEncoder::default()
        .module(module_bytes)?
        .validate(true)
        .encode()
}
```

#### 生成されるstub.mbt（例: greet, lint の2コマンド）

```moonbit
pub fn list_commands() -> Array[@types.CommandMeta] {
  [@greet.meta(), @lint.meta()]
}

pub fn run(name : String, argv : Array[String]) -> Result[UInt, @types.CommandError] {
  let meta_0 = @greet.meta()
  if name == meta_0.name { return @greet.run(argv) }
  for i = 0; i < meta_0.aliases.length(); i = i + 1 {
    if name == meta_0.aliases[i] { return @greet.run(argv) }
  }

  let meta_1 = @lint.meta()
  if name == meta_1.name { return @lint.run(argv) }
  for i = 0; i < meta_1.aliases.length(); i = i + 1 {
    if name == meta_1.aliases[i] { return @lint.run(argv) }
  }

  Err(@types.CommandError::UnknownCommand(name))
}
```

#### 実装タスク

1. MoonBitコード生成器（stub.mbt, moon.pkg.json, moon.mod.json）
2. WIT生成器（world.wit）
3. `moon build` 呼び出し
4. wit-component crate で component 化

#### 依存

- `moon` コマンドがPATHにあること（MoonBitビルド用）
- wasm-tools は**不要**（wit-component crateで代替）

---

### Phase 2c: フルビルドパイプライン

**目標**: プラグインのビルドも含めた完全自動化

```
wacli.toml + ソース → 全コンポーネントビルド → 合成 → tool.wasm
```

#### wacli.toml 拡張

```toml
[[command]]
name = "greet"
source = "plugins/greeter"  # ソースディレクトリ → moon build + component化
# または
plugin = "plugins/greeter/greeter.component.wasm"  # ビルド済み
```

#### 実装タスク

1. ソースディレクトリの検出
2. moon build 呼び出し
3. wit-component で component 化
4. 並列ビルド対応

---

## Cargo.toml 更新

```toml
[dependencies]
# WAC crates
wac-parser = { version = "0.9.0-dev", default-features = false }
wac-resolver = { version = "0.9.0-dev", default-features = false, features = ["wit"] }
wac-graph = "0.9.0-dev"
wac-types = "0.9.0-dev"

# WIT/Component (wasm-tools CLI 不要にする)
wit-parser = "0.244.0"
wit-component = "0.244.0"

# Manifest
toml = "0.8"
serde = { version = "1.0", features = ["derive"] }

# 既存
clap = { version = "4.5", features = ["derive"] }
anyhow = "1.0"
miette = { version = "7.2", features = ["fancy"] }
tokio = { version = "1.45", features = ["macros", "rt-multi-thread", "process"] }
indexmap = "2.2"
log = "0.4"
env_logger = "0.11"
```

---

## Phase 2 の実装優先度

```
Phase 2a → Phase 2b → Phase 2c
   ↓          ↓          ↓
  基本動作    自動生成    フルパイプライン
```

## 成果物

### wacli コマンド

| コマンド | Phase | 説明 |
|---------|-------|------|
| `wacli compose` | 1 (完了) | WAC直接合成 |
| `wacli plug` | 1 (完了) | 簡易合成 |
| `wacli init` | 2a | プロジェクト初期化 |
| `wacli build` | 2a | manifest→合成 |

### ファイル構成（2b完了後）

```
my-tool/
├── wacli.toml              # ユーザー編集
├── plugins/
│   └── greet/
│       ├── wit/world.wit
│       └── gen/            # wit-bindgen生成
├── .wacli/                 # wacli生成（gitignore推奨）
│   ├── registry/           # 自動生成されたregistry
│   │   ├── wit/world.wit
│   │   ├── moon.mod.json
│   │   └── gen/
│   └── compose.wac         # 生成されたWAC（デバッグ用）
└── dist/
    └── my-tool.component.wasm
```
