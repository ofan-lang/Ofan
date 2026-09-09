use std::process::Command;

const SMOKE_TEST: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/smoke_test.ofn");

// TODO(daily_total): wire up examples/daily_total.ofn as a golden/integration test once
// the collaborator shares the source file. Pattern: compile with `ofan build`, run the
// binary, assert exit code matches the predicted checksum value (mod 256).
// Ref: fix/windows-linker PR discussion.

#[test]
fn diag_check_ok() {
    let bin = env!("CARGO_BIN_EXE_ofan");
    let output = Command::new(bin)
        .args(["check", SMOKE_TEST])
        .output()
        .expect("failed to run ofan binary");
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    insta::assert_snapshot!(stderr.trim());
}

#[test]
#[cfg(not(feature = "codegen"))]
fn diag_codegen_disabled_build() {
    let bin = env!("CARGO_BIN_EXE_ofan");
    let output = Command::new(bin)
        .args(["build", SMOKE_TEST])
        .output()
        .expect("failed to run ofan binary");
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    insta::assert_snapshot!(stderr.trim());
}
