use std::process::Command;

#[test]
fn json_mode_reports_runtime_errors_as_json() {
    const MISSING_KEY_ENV: &str = "CINNABAR_TEST_MISSING_KEY";
    let output = Command::new(env!("CARGO_BIN_EXE_ckb-cinnabar"))
        .args(["--json", "--privkey-env", MISSING_KEY_ENV, "list"])
        .env_remove(MISSING_KEY_ENV)
        .output()
        .expect("run ckb-cinnabar");

    assert!(!output.status.success());
    assert!(
        output.stderr.is_empty(),
        "JSON mode must not emit unstructured stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["ok"], false);
    assert_eq!(response["operation"], "list");
    assert_eq!(response["dry_run"], false);
    assert_eq!(response["error"]["kind"], "configuration");
    assert!(response["error"]["message"]
        .as_str()
        .is_some_and(|message| message.contains(MISSING_KEY_ENV)));
}

#[test]
fn json_mode_reports_argument_errors_as_json() {
    let output = Command::new(env!("CARGO_BIN_EXE_ckb-cinnabar"))
        .args(["--json", "list", "--unknown-option"])
        .output()
        .expect("run ckb-cinnabar");

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(2));
    assert!(
        output.stderr.is_empty(),
        "JSON mode must not emit clap text on stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(response["ok"], false);
    assert_eq!(response["operation"], "list");
    assert_eq!(response["error"]["kind"], "invalid_input");
    assert!(response["error"]["message"]
        .as_str()
        .is_some_and(|message| message.contains("--unknown-option")));
}
