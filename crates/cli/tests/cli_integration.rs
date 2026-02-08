use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn make_temp_dir(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before UNIX_EPOCH")
        .as_nanos();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("wacli-integ-{prefix}-{pid}-{nanos}"));
    fs::create_dir_all(&dir).expect("failed to create temp dir");
    dir
}

fn repo_root() -> &'static Path {
    // crates/cli -> crates -> <repo root>
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("unexpected CARGO_MANIFEST_DIR layout")
}

fn wacli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_wacli"))
}

#[test]
fn help_works() {
    let out = wacli()
        .arg("--help")
        .output()
        .expect("failed to run wacli --help");
    assert!(
        out.status.success(),
        "wacli --help failed:\nstatus: {}\nstderr:\n{}",
        out.status,
        String::from_utf8_lossy(&out.stderr),
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("wacli") && stdout.contains("init") && stdout.contains("build"),
        "unexpected help output:\n{stdout}"
    );
}

#[test]
fn init_creates_expected_layout() {
    let dir = make_temp_dir("init-layout");

    let out = wacli()
        .arg("init")
        .arg(&dir)
        .output()
        .expect("failed to run wacli init");
    assert!(
        out.status.success(),
        "wacli init failed:\nstatus: {}\nstderr:\n{}",
        out.status,
        String::from_utf8_lossy(&out.stderr),
    );

    assert!(dir.join("defaults").is_dir(), "defaults/ not created");
    assert!(dir.join("commands").is_dir(), "commands/ not created");
    assert!(dir.join("wit").is_dir(), "wit/ not created");
    assert!(
        dir.join("wit/types.wit").is_file(),
        "wit/types.wit not created"
    );
    assert!(
        dir.join("wacli.json").is_file(),
        "wacli.json (manifest) not created"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn build_composes_component_from_fixture_plugin() {
    let dir = make_temp_dir("build-component");
    let defaults_dir = dir.join("defaults");
    let commands_dir = dir.join("commands");
    fs::create_dir_all(&defaults_dir).expect("failed to create defaults dir");
    fs::create_dir_all(&commands_dir).expect("failed to create commands dir");

    // Framework components (host/core) are shipped in-repo.
    fs::copy(
        repo_root().join("components/host.component.wasm"),
        defaults_dir.join("host.component.wasm"),
    )
    .expect("failed to copy host.component.wasm");
    fs::copy(
        repo_root().join("components/core.component.wasm"),
        defaults_dir.join("core.component.wasm"),
    )
    .expect("failed to copy core.component.wasm");

    // A tiny plugin component fixture built from test-build/commands/greet.
    fs::copy(
        repo_root().join("testdata/greet.component.wasm"),
        commands_dir.join("greet.component.wasm"),
    )
    .expect("failed to copy greet.component.wasm fixture");

    let output_path = dir.join("out.component.wasm");

    let out = wacli()
        .current_dir(&dir)
        .arg("build")
        .arg("--name")
        .arg("example:test-cli")
        .arg("--version")
        .arg("0.1.0")
        .arg("--output")
        .arg(&output_path)
        .arg("--defaults-dir")
        .arg(&defaults_dir)
        .arg("--commands-dir")
        .arg(&commands_dir)
        .output()
        .expect("failed to run wacli build");
    assert!(
        out.status.success(),
        "wacli build failed:\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
        out.status,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );

    let bytes = fs::read(&output_path).expect("failed to read output component");
    assert!(
        bytes.len() >= 8,
        "output component is too small: {} bytes",
        bytes.len()
    );
    assert_eq!(&bytes[0..4], b"\0asm", "output is not a wasm binary");
    let payload = wasmparser::Parser::new(0)
        .parse_all(&bytes)
        .next()
        .expect("wasmparser produced no payloads")
        .expect("failed to parse output wasm");
    let wasmparser::Payload::Version { encoding, .. } = payload else {
        panic!("expected wasm version payload first");
    };
    assert_eq!(
        encoding,
        wasmparser::Encoding::Component,
        "output is not a wasm component"
    );

    let _ = fs::remove_dir_all(&dir);
}
