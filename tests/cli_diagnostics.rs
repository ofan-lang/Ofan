use std::process::Command;

const SMOKE_TEST: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/smoke_test.ofan");
const BITWISE_OPS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/bitwise_ops.ofan");
const SELF_RETURN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/self_return.ofan");
const REF_TYPES_GUARD: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/examples/ref_types_guard.ofan");

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

#[test]
#[cfg(feature = "codegen")]
fn codegen_bitwise_ops_build_and_run() {
    let bin = env!("CARGO_BIN_EXE_ofan");
    let out_dir = std::env::temp_dir();
    let out_bin = out_dir.join("bitwise_ops_test");
    let build_out = Command::new(bin)
        .args(["build", BITWISE_OPS, "-o"])
        .arg(&out_bin)
        .output()
        .expect("failed to run ofan build");
    assert!(
        build_out.status.success(),
        "build failed:\n{}",
        String::from_utf8_lossy(&build_out.stderr)
    );
    let run_status = Command::new(&out_bin)
        .status()
        .expect("failed to run compiled binary");
    let _ = std::fs::remove_file(&out_bin);
    assert_eq!(run_status.code(), Some(102));
}

#[test]
#[cfg(feature = "codegen")]
fn codegen_self_return_build_and_run() {
    let bin = env!("CARGO_BIN_EXE_ofan");
    let out_dir = std::env::temp_dir();
    let out_bin = out_dir.join("self_return_test");
    let build_out = Command::new(bin)
        .args(["build", SELF_RETURN, "-o"])
        .arg(&out_bin)
        .output()
        .expect("failed to run ofan build");
    assert!(
        build_out.status.success(),
        "build failed:\n{}",
        String::from_utf8_lossy(&build_out.stderr)
    );
    let run_status = Command::new(&out_bin)
        .status()
        .expect("failed to run compiled binary");
    let _ = std::fs::remove_file(&out_bin);
    assert_eq!(run_status.code(), Some(7));
}

#[test]
#[cfg(feature = "codegen")]
fn codegen_ref_types_guard_clean_error() {
    let bin = env!("CARGO_BIN_EXE_ofan");
    let build_out = Command::new(bin)
        .args(["build", REF_TYPES_GUARD])
        .output()
        .expect("failed to run ofan build");
    assert!(
        !build_out.status.success(),
        "expected build to fail for &T param, but it succeeded"
    );
    let stderr = String::from_utf8_lossy(&build_out.stderr).to_string();
    insta::assert_snapshot!(stderr.trim());
}
