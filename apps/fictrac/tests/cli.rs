use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
type Result = std::result::Result<(), Box<dyn std::error::Error>>;
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> std::io::Result<Self> {
        let p = std::env::temp_dir().join(format!(
            "fictrac-cli-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p)?;
        Ok(Self(p))
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_fictrac"))
}
#[test]
fn demo_proves_full_pipeline_and_refuses_overwrite() -> Result {
    let temp = Temp::new()?;
    let p = temp.0.join("nested/demo");
    let out = cli()
        .args(["demo", "--frames", "8", "--output"])
        .arg(&p)
        .output()?;
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let oracle: serde_json::Value = serde_json::from_slice(&fs::read(p.join("oracle.json"))?)?;
    assert_eq!(oracle["passed"], true);
    let original = fs::read(p.join("tracking/tracking.dat"))?;
    let result = cli()
        .args(["demo", "--frames", "8", "--output"])
        .arg(&p)
        .output()?;
    assert!(!result.status.success());
    assert_eq!(fs::read(p.join("tracking/tracking.dat"))?, original);
    assert_eq!(String::from_utf8(original)?.lines().count(), 8);
    Ok(())
}
#[test]
fn tracks_original_config_and_exports_complete_artifacts() -> Result {
    let temp = Temp::new()?;
    let p = temp.0.join("sample");
    let config =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/upstream-config.txt");
    let result = cli()
        .args(["track", "--config"])
        .arg(&config)
        .args(["--max-frames", "3", "--output"])
        .arg(&p)
        .output()?;
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let summary: serde_json::Value = serde_json::from_slice(&fs::read(p.join("summary.json"))?)?;
    assert_eq!(summary["frames"], 3);
    assert_eq!(summary["rejected"], 0);
    for entry in fs::read_dir(p)? {
        assert!(!entry?.file_name().to_string_lossy().ends_with(".partial"));
    }
    Ok(())
}
#[test]
fn malformed_calibration_leaves_no_output_file() -> Result {
    let temp = Temp::new()?;
    let p = temp.0.join("bad.json");
    let result = cli()
        .args([
            "calibrate",
            "--width",
            "160",
            "--height",
            "160",
            "--vfov",
            "45",
            "--points",
            "[[1,1],[1,1],[1,1]]",
            "--output",
        ])
        .arg(&p)
        .output()?;
    assert!(!result.status.success());
    assert!(!p.exists());
    Ok(())
}

#[test]
fn decode_failure_retains_partial_files_without_success_summary() -> Result {
    let temp = Temp::new()?;
    let input = temp.0.join("truncated.y4m");
    let config = temp.0.join("config.json");
    let output = temp.0.join("run");
    let c = fictrac_core::Config::default();
    fs::write(&config, serde_json::to_vec(&c)?)?;
    let mut data = b"YUV4MPEG2 W160 H160 F60:1 Ip Cmono\nFRAME\n".to_vec();
    data.extend(vec![128u8; 160 * 160]);
    data.extend(b"FRAME\nshort");
    fs::write(&input, data)?;
    let result = cli()
        .args(["track", "--config"])
        .arg(config)
        .arg("--input")
        .arg(input)
        .arg("--output")
        .arg(&output)
        .output()?;
    assert!(!result.status.success());
    assert!(output.join("tracking.dat.partial").exists());
    assert!(!output.join("tracking.dat").exists());
    assert!(!output.join("summary.json").exists());
    Ok(())
}

#[test]
fn default_demo_works_in_a_fresh_directory() -> Result {
    let temp = Temp::new()?;
    let result = cli()
        .current_dir(&temp.0)
        .args(["demo", "--frames", "5"])
        .output()?;
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(temp.0.join("demo-run/oracle.json").exists());
    Ok(())
}

#[test]
fn legacy_video_invocation_and_lossless_config_edit_work() -> Result {
    let temp = Temp::new()?;
    let config =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/upstream-config.txt");
    let edited = temp.0.join("edited.txt");
    let out = cli()
        .arg("config")
        .arg("--config")
        .arg(&config)
        .args([
            "--set",
            "thr_ratio=1.2",
            "--set",
            "agent_note=literal $(touch ignored)",
            "--output",
        ])
        .arg(&edited)
        .output()?;
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = fs::read_to_string(edited)?;
    assert!(text.contains("agent_note"));
    assert!(text.contains("$(touch ignored)"));
    assert!(!temp.0.join("ignored").exists());
    let out = cli()
        .arg(config)
        .args(["--max-frames", "2", "--output"])
        .arg(temp.0.join("legacy-run"))
        .output()?;
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        fs::read_to_string(temp.0.join("legacy-run/all-frames.dat"))?
            .lines()
            .count(),
        2
    );
    Ok(())
}
#[test]
fn cli_agent_and_mcp_keep_stdout_machine_readable() -> Result {
    use std::io::Write;
    use std::process::Stdio;
    for (mode, input) in [
        (
            "agent",
            serde_json::json!({"id":"discovery","command":"discover"}),
        ),
        (
            "mcp",
            serde_json::json!({"jsonrpc":"2.0","id":"init","method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"cli-test","version":"1"}}}),
        ),
    ] {
        let mut child = cli()
            .arg(mode)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let mut stdin = child.stdin.take().ok_or("missing stdin")?;
        writeln!(stdin, "{input}")?;
        drop(stdin);
        let output = child.wait_with_output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout)?;
        assert_eq!(stdout.lines().count(), 1);
        let _: serde_json::Value = serde_json::from_str(stdout.trim())?;
    }
    Ok(())
}

#[test]
fn legacy_default_directory_honors_output_prefix_and_input_stem() -> Result {
    let temp = Temp::new()?;
    let example = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let mut doc = fictrac_io::ConfigDocument::parse(fs::read_to_string(
        example.join("upstream-config.txt"),
    )?)?;
    doc.set(
        "src_fn",
        &fs::canonicalize(example.join("sample.mp4"))?.to_string_lossy(),
    )?;
    doc.set("output_fn", "nested/animal-session")?;
    let config = temp.0.join("config.txt");
    fs::write(&config, doc.text())?;
    let out = cli()
        .current_dir(&temp.0)
        .arg(&config)
        .args(["--max-frames", "2"])
        .output()?;
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let directories = fs::read_dir(temp.0.join("nested"))?.collect::<std::io::Result<Vec<_>>>()?;
    assert_eq!(directories.len(), 1);
    assert!(
        directories[0]
            .file_name()
            .to_string_lossy()
            .starts_with("animal-session-")
    );
    assert!(directories[0].path().join("tracking.dat").is_file());
    doc.set("output_fn", "")?;
    fs::write(&config, doc.text())?;
    let out = cli()
        .current_dir(&temp.0)
        .arg(&config)
        .args(["--max-frames", "2"])
        .output()?;
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        fs::read_dir(&temp.0)?
            .filter_map(|entry| entry.ok())
            .any(
                |entry| entry.file_name().to_string_lossy().starts_with("sample-")
                    && entry.path().is_dir()
            )
    );
    Ok(())
}
