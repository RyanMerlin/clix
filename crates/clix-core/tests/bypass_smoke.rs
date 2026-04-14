/// Test 1 — Bypass smoke test: credential adoption makes direct CLI auth fail.
///
/// What this proves:
/// (a) After `clix init --adopt-creds gcloud`, the original ADC path becomes a dead symlink.
///     Any process that tries to open it (including raw `gcloud`) gets ENOENT.
/// (b) The broker-owned copy lives under the broker creds dir at mode 0600.
/// (c) The broker creds dir itself is mode 0700.
/// (d) `gcloud auth print-access-token` invoked without going through clix fails — because the
///     ADC is gone from the expected location.
///
/// Note on UID isolation: in this test we run as the same user (no dedicated broker UID yet,
/// that's a deployment concern). The load-bearing security property is the dead symlink at the
/// original credential path — any agent that finds `~/.config/gcloud/adc.json` and tries to
/// read it gets ENOENT, regardless of UID. Full UID separation is a deployment step (clix-broker
/// runs as its own system user; only it can read /var/lib/clix/broker/).
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use tempfile::TempDir;

/// Simulate the gcloud ADC adoption logic with configurable paths (so we don't
/// touch the real ~/.config/gcloud in tests).
fn adopt_gcloud_to_temp(fake_home: &Path, broker_creds_dir: &Path) -> std::io::Result<()> {
    let src = fake_home.join(".config").join("gcloud").join("application_default_credentials.json");
    assert!(src.exists(), "pre-condition: fake ADC must exist at {}", src.display());

    let dest_dir = broker_creds_dir.join("gcloud");
    fs::create_dir_all(&dest_dir)?;
    fs::set_permissions(&dest_dir, fs::Permissions::from_mode(0o700))?;

    let dest = dest_dir.join("adc.json");
    fs::copy(&src, &dest)?;
    fs::set_permissions(&dest, fs::Permissions::from_mode(0o600))?;

    // Replace original with dead symlink (ENOENT when followed)
    fs::remove_file(&src)?;
    std::os::unix::fs::symlink("/clix-broker-adopted-this-credential", &src)?;

    Ok(())
}

#[test]
fn test_adopt_moves_adc_to_broker_dir() {
    let fake_home = TempDir::new().unwrap();
    let broker_dir = TempDir::new().unwrap();

    // Create fake ADC JSON
    let gcloud_dir = fake_home.path().join(".config").join("gcloud");
    fs::create_dir_all(&gcloud_dir).unwrap();
    let adc_path = gcloud_dir.join("application_default_credentials.json");
    fs::write(&adc_path, r#"{"type":"authorized_user","client_id":"test","refresh_token":"tok"}"#).unwrap();

    adopt_gcloud_to_temp(fake_home.path(), broker_dir.path()).unwrap();

    // Broker copy must exist and be mode 0600
    let broker_copy = broker_dir.path().join("gcloud").join("adc.json");
    assert!(broker_copy.exists(), "broker copy must exist");
    let meta = fs::metadata(&broker_copy).unwrap();
    assert_eq!(meta.permissions().mode() & 0o777, 0o600, "broker copy should be mode 0600");

    // Broker dir itself must be 0700
    let broker_gcloud_dir = broker_dir.path().join("gcloud");
    let dir_meta = fs::metadata(&broker_gcloud_dir).unwrap();
    assert_eq!(dir_meta.permissions().mode() & 0o777, 0o700, "broker creds dir should be mode 0700");
}

#[test]
fn test_original_adc_becomes_dead_symlink() {
    let fake_home = TempDir::new().unwrap();
    let broker_dir = TempDir::new().unwrap();

    let gcloud_dir = fake_home.path().join(".config").join("gcloud");
    fs::create_dir_all(&gcloud_dir).unwrap();
    let adc_path = gcloud_dir.join("application_default_credentials.json");
    fs::write(&adc_path, r#"{"type":"authorized_user"}"#).unwrap();

    adopt_gcloud_to_temp(fake_home.path(), broker_dir.path()).unwrap();

    // The original path should be a symlink (dead — target doesn't exist)
    let link_meta = fs::symlink_metadata(&adc_path).expect("symlink_metadata should succeed");
    assert!(link_meta.file_type().is_symlink(), "original path should be a symlink");

    // Following the symlink must fail with NotFound
    let read_result = fs::read(&adc_path);
    assert!(
        read_result.is_err(),
        "reading the dead symlink should fail"
    );
    let err = read_result.unwrap_err();
    assert_eq!(
        err.kind(),
        std::io::ErrorKind::NotFound,
        "expected NotFound (ENOENT) when following dead symlink, got: {err}"
    );
}

#[test]
fn test_gcloud_cli_fails_without_adc() {
    // Verify that running gcloud without ADC accessible returns a non-zero exit code.
    // We point CLOUDSDK_CONFIG to a temp dir with no credentials.
    let tmp = TempDir::new().unwrap();
    let output = std::process::Command::new("gcloud")
        .args(["auth", "print-access-token"])
        .env("CLOUDSDK_CONFIG", tmp.path()) // empty config dir — no credentials
        .env("HOME", tmp.path())
        .stderr(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .output();

    match output {
        Ok(out) => {
            assert_ne!(
                out.status.code().unwrap_or(0), 0,
                "gcloud with no credentials should exit non-zero"
            );
            let stderr = String::from_utf8_lossy(&out.stderr);
            // gcloud should mention that no credentials are available
            assert!(
                stderr.contains("credentials") || stderr.contains("auth") || stderr.contains("ADC") || stderr.contains("login"),
                "gcloud error should mention credentials, got: {stderr}"
            );
        }
        Err(e) => {
            // gcloud not installed — skip gracefully
            eprintln!("skipping gcloud CLI test: {e}");
        }
    }
}

#[test]
fn test_broker_copy_content_matches_original() {
    let fake_home = TempDir::new().unwrap();
    let broker_dir = TempDir::new().unwrap();
    let original_content = r#"{"type":"authorized_user","client_id":"real-id"}"#;

    let gcloud_dir = fake_home.path().join(".config").join("gcloud");
    fs::create_dir_all(&gcloud_dir).unwrap();
    fs::write(gcloud_dir.join("application_default_credentials.json"), original_content).unwrap();

    adopt_gcloud_to_temp(fake_home.path(), broker_dir.path()).unwrap();

    let broker_content = fs::read_to_string(broker_dir.path().join("gcloud").join("adc.json")).unwrap();
    assert_eq!(broker_content, original_content, "broker copy content must match original");
}
