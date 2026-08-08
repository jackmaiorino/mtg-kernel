#![cfg(feature = "native-training-store-v2-production")]

use std::process::Command;

fn valid_validate_store_arguments() -> Vec<String> {
    vec![
        "validate-store".to_owned(),
        "--store-root".to_owned(),
        "C:\\store".to_owned(),
        "--source-root".to_owned(),
        "C:\\source".to_owned(),
        "--expected-run-sha256".to_owned(),
        "a".repeat(64),
        "--expected-generation-index".to_owned(),
        "0".to_owned(),
        "--expected-head-sha256".to_owned(),
        "b".repeat(64),
    ]
}

#[test]
fn valid_command_reaches_only_the_exact_platform_stub() {
    let output = Command::new(env!("CARGO_BIN_EXE_mtg-kernel-native"))
        .args(valid_validate_store_arguments())
        .output()
        .unwrap();
    assert!(output.stdout.is_empty());

    #[cfg(target_os = "windows")]
    {
        assert_eq!(output.status.code(), Some(3));
        assert_eq!(
            output.stderr,
            b"mtg-kernel-native-cli-v2-command-not-implemented\n"
        );
    }
    #[cfg(not(target_os = "windows"))]
    {
        assert_eq!(output.status.code(), Some(5));
        assert_eq!(
            output.stderr,
            b"native-training-store-v2-unsupported-platform\n"
        );
    }
}

#[test]
fn invalid_syntax_precedes_the_platform_gate() {
    let output = Command::new(env!("CARGO_BIN_EXE_mtg-kernel-native"))
        .args(["validate-store", "--unknown", "value"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(output.stderr, b"mtg-kernel-native-cli-v2-invalid-syntax\n");
}
