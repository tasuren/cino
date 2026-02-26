use std::fs;
use std::process::Command;

#[test]
fn test_cli_smoke() {
    // 1. Setup sample file
    let sample = "enum Action = | A\nenum Result<T, E> = | Ok { v: T } | Err { e: E }\nevent E = | Ev\nquery Q = | Qu\nupdate(s: Int, e: E) -> (Int, List<Action>) { (s, []) }\nquery(s: Int, q: Q) -> Result<Int, Int> { Ok { v: s } }";
    fs::write("smoke_sample.cino", sample).unwrap();

    // Ensure binary is built
    let build_status = Command::new("cargo")
        .args(&["build", "-p", "cino-cli", "--bin", "cino"])
        .status()
        .expect("cargo build failed");
    assert!(build_status.success());

    // Binary path: cargo build usually puts it in target/debug/cino relative to workspace root.
    // Since we are running from crates/cino-cli, we can try target/debug/cino in the workspace root.
    let bin = "../../target/debug/cino";

    // 2. Test check
    let status = Command::new(bin)
        .args(&["check", "--file", "smoke_sample.cino"])
        .status()
        .expect("cino check failed to run");
    assert!(status.success());

    // 3. Test run update
    let output = Command::new(bin)
        .args(&[
            "run",
            "update",
            "--file",
            "smoke_sample.cino",
            "--state",
            "0",
            "--event",
            "{\"$tag\": \"Ev\", \"$fields\": {}}",
        ])
        .output()
        .expect("cino run update failed to run");
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("\"next_state\": 0"));

    // 4. Test run query
    let output = Command::new(bin)
        .args(&[
            "run",
            "query",
            "--file",
            "smoke_sample.cino",
            "--state",
            "42",
            "--query",
            "{\"$tag\": \"Qu\", \"$fields\": {}}",
        ])
        .output()
        .expect("cino run query failed to run");
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("\"$tag\": \"Ok\""));
    assert!(stdout.contains("\"v\": 42"));

    // 5. Test docgen
    let status = Command::new(bin)
        .args(&[
            "docgen",
            "--file",
            "smoke_sample.cino",
            "--out",
            "smoke_docs",
        ])
        .status()
        .expect("cino docgen failed to run");
    assert!(status.success());
    assert!(fs::metadata("smoke_docs/spec.md").is_ok());

    // Cleanup
    let _ = fs::remove_file("smoke_sample.cino");
    let _ = fs::remove_dir_all("smoke_docs");
}
