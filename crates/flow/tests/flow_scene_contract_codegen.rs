//! Generated scene bindings must remain byte-for-byte synchronized with Rust DTOs.

use std::process::Command;

#[test]
fn committed_flow_scene_v9_contract_matches_rust() {
    let output = Command::new(env!("CARGO_BIN_EXE_export_flow_scene_contract"))
        .arg("--check")
        .output()
        .expect("run flow scene contract checker");
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
