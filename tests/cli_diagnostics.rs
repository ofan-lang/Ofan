use std::process::{Command, Stdio};

const SMOKE_TEST: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/smoke_test.ofan");
const BITWISE_OPS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/bitwise_ops.ofan");
const SELF_RETURN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/self_return.ofan");
const REF_TYPES_GUARD: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/ref_types_guard.ofan");
const DIV_TRAP: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/div_trap.ofan");
const MOD_TRAP: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/mod_trap.ofan");
const SHIFT_TRAP: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/shift_trap.ofan");

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

/// Helper: build `src`, run the binary, return (exit_code, stderr).
#[cfg(all(feature = "codegen", windows))]
fn build_and_run_capture_stderr(src: &str, bin_name: &str) -> (i32, String) {
    let ofan = env!("CARGO_BIN_EXE_ofan");
    let out_bin = std::env::temp_dir().join(bin_name);
    let build_out = Command::new(ofan)
        .args(["build", src, "-o"])
        .arg(&out_bin)
        .output()
        .expect("failed to run ofan build");
    assert!(
        build_out.status.success(),
        "build failed:\n{}",
        String::from_utf8_lossy(&build_out.stderr)
    );
    let run_out = Command::new(&out_bin)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("failed to run compiled binary");
    let _ = std::fs::remove_file(&out_bin);
    let code = run_out.status.code().unwrap_or(-1);
    let stderr = String::from_utf8_lossy(&run_out.stderr).to_string();
    (code, stderr)
}

#[test]
#[cfg(all(feature = "codegen", windows))]
fn codegen_div_trap_prints_message() {
    let (code, stderr) = build_and_run_capture_stderr(DIV_TRAP, "div_trap_test.exe");
    assert_ne!(code, 0, "expected non-zero exit for division by zero");
    assert!(
        stderr.contains("runtime error: division by zero"),
        "expected 'runtime error: division by zero' in stderr, got: {stderr:?}"
    );
}

#[test]
#[cfg(all(feature = "codegen", windows))]
fn codegen_mod_trap_prints_message() {
    let (code, stderr) = build_and_run_capture_stderr(MOD_TRAP, "mod_trap_test.exe");
    assert_ne!(code, 0, "expected non-zero exit for modulo by zero");
    assert!(
        stderr.contains("runtime error: modulo by zero"),
        "expected 'runtime error: modulo by zero' in stderr, got: {stderr:?}"
    );
}

#[test]
#[cfg(all(feature = "codegen", windows))]
fn codegen_shift_trap_prints_message() {
    let (code, stderr) = build_and_run_capture_stderr(SHIFT_TRAP, "shift_trap_test.exe");
    assert_ne!(code, 0, "expected non-zero exit for shift out of range");
    assert!(
        stderr.contains("runtime error: shift amount out of range"),
        "expected 'runtime error: shift amount out of range' in stderr, got: {stderr:?}"
    );
}
