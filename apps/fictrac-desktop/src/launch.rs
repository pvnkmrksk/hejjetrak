//! Linux startup isolation: bounded graphics retries and persistent diagnostics.
//! Once a window has rendered, a failure exits visibly instead of restarting a run.
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::{fs::OpenOptionsExt, process::ExitStatusExt},
    path::{Path, PathBuf},
    process::{Command, ExitCode, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const CHILD: &str = "HEJJETRAC_LAUNCH_CHILD";
pub const READY: &str = "Hejjetrac desktop ready";
const STARTUP_TIMEOUT: Duration = Duration::from_secs(20);
const LOG_LIMIT: usize = 2 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Automatic,
    Vulkan,
    Software,
}

fn modes(safe: bool, explicit_gpu: bool) -> Vec<Mode> {
    if safe {
        vec![Mode::Software]
    } else if explicit_gpu {
        vec![Mode::Automatic]
    } else {
        vec![Mode::Automatic, Mode::Vulkan, Mode::Software]
    }
}

fn configure(command: &mut Command, mode: Mode) {
    command.env(CHILD, "1");
    if std::env::var_os("RUST_BACKTRACE").is_none() {
        command.env("RUST_BACKTRACE", "1");
    }
    match mode {
        Mode::Automatic => {}
        Mode::Vulkan => {
            command.env("WGPU_BACKEND", "vulkan");
        }
        Mode::Software => {
            command
                .env("WGPU_BACKEND", "gl")
                .env_remove("WGPU_ADAPTER_NAME")
                .env("LIBGL_ALWAYS_SOFTWARE", "1")
                .env("GALLIUM_DRIVER", "llvmpipe");
            let mesa = Path::new("/usr/share/glvnd/egl_vendor.d/50_mesa.json");
            if mesa.is_file() {
                command.env("__EGL_VENDOR_LIBRARY_FILENAMES", mesa);
            }
        }
    }
    if mode != Mode::Automatic {
        command.env("HEJJETRAC_STARTUP_MODE", format!("{mode:?}"));
    } else {
        command.env_remove("HEJJETRAC_STARTUP_MODE");
    }
}

fn state_directory(env: impl Fn(&str) -> Option<PathBuf>) -> PathBuf {
    env("XDG_STATE_HOME")
        .filter(|p| p.is_absolute())
        .or_else(|| {
            env("HOME")
                .filter(|p| p.is_absolute())
                .map(|p| p.join(".local/state"))
        })
        .unwrap_or_else(std::env::temp_dir)
        .join("hejjetrac/logs")
}

fn directory() -> PathBuf {
    state_directory(|key| std::env::var_os(key).map(PathBuf::from))
}

struct Journal {
    file: File,
    written: usize,
    limit: usize,
    summary_written: usize,
}
impl Journal {
    fn write(&mut self, bytes: &[u8]) {
        let count = bytes.len().min(self.limit.saturating_sub(self.written));
        if count > 0
            && let Ok(written) = self.file.write(&bytes[..count])
        {
            self.written += written;
            let _ = self.file.flush();
        }
    }
    fn summary(&mut self, bytes: &[u8]) {
        let count = bytes
            .len()
            .min((16 * 1024usize).saturating_sub(self.summary_written));
        if count > 0
            && let Ok(written) = self.file.write(&bytes[..count])
        {
            self.summary_written += written;
            let _ = self.file.flush();
        }
    }
}
type Log = Option<Arc<Mutex<Journal>>>;

fn open_log(directory: &Path) -> Result<(PathBuf, Log), String> {
    fs::create_dir_all(directory)
        .map_err(|e| format!("Cannot create {}: {e}", directory.display()))?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let path = directory.join(format!("session-{stamp}-{}.log", std::process::id()));
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .map_err(|e| format!("Cannot open {}: {e}", path.display()))?;
    Ok((
        path,
        Some(Arc::new(Mutex::new(Journal {
            file,
            written: 0,
            limit: LOG_LIMIT,
            summary_written: 0,
        }))),
    ))
}

fn log(log: &Log, text: &str) {
    if let Some(log) = log
        && let Ok(mut log) = log.lock()
    {
        log.summary(text.as_bytes());
    }
}

fn environment_report() -> String {
    let mut text = format!(
        "Hejjetrac {} · {} {}\n",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    if let Ok(os) = fs::read_to_string("/etc/os-release") {
        for line in os
            .lines()
            .filter(|s| s.starts_with("PRETTY_NAME=") || s.starts_with("VERSION_ID="))
        {
            text.push_str(line);
            text.push('\n');
        }
    }
    for key in [
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "XDG_SESSION_TYPE",
        "WINIT_UNIX_BACKEND",
        "WGPU_BACKEND",
        "WGPU_ADAPTER_NAME",
        "LIBGL_ALWAYS_SOFTWARE",
        "VK_DRIVER_FILES",
        "VK_ICD_FILENAMES",
        "__EGL_VENDOR_LIBRARY_FILENAMES",
    ] {
        let value = std::env::var_os(key)
            .map(|s| s.to_string_lossy().chars().take(1024).collect::<String>())
            .unwrap_or_else(|| "<unset>".into());
        text.push_str(&format!("{key}={value}\n"));
    }
    text
}

pub fn print_diagnostics() {
    println!("{}Logs: {}", environment_report(), directory().display());
    if let Ok(entries) = fs::read_dir(directory()) {
        let mut paths = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .is_some_and(|s| s.to_string_lossy().starts_with("session-"))
            })
            .collect::<Vec<_>>();
        paths.sort();
        for path in paths.iter().rev().take(5) {
            println!("{}", path.display());
        }
    }
}

#[derive(Clone, Default)]
struct Observation {
    ready: Arc<AtomicBool>,
    graphics_error: Arc<AtomicBool>,
}
impl Observation {
    fn inspect(&self, text: &str) {
        if text.contains(READY) {
            self.ready.store(true, Ordering::Release);
        }
        let text = text.to_ascii_lowercase();
        if [
            "wgpu",
            "winit",
            "no graphics adapter",
            "egl_bad",
            "eglinitialize",
            "vk_error_",
            "failed to create surface",
        ]
        .iter()
        .any(|s| text.contains(s))
        {
            self.graphics_error.store(true, Ordering::Release);
        }
    }
}

fn pump(mut input: impl Read, log: Log, observation: Observation, stderr: bool) {
    let mut buffer = [0u8; 4096];
    let mut tail = Vec::new();
    loop {
        let count = match input.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        let bytes = &buffer[..count];
        tail.extend_from_slice(bytes);
        observation.inspect(&String::from_utf8_lossy(&tail));
        if tail.len() > 256 {
            tail.drain(..tail.len() - 256);
        }
        if let Some(log) = &log
            && let Ok(mut log) = log.lock()
        {
            log.write(bytes);
        }
        if stderr {
            let _ = std::io::stderr().write_all(bytes);
        } else {
            let _ = std::io::stdout().write_all(bytes);
        }
    }
}

struct Outcome {
    success: bool,
    signal: Option<i32>,
    code: Option<i32>,
    ready: bool,
    graphics_error: bool,
    timed_out: bool,
}
struct OwnedChild(std::process::Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if self.0.try_wait().is_ok_and(|status| status.is_none()) {
            let _ = self.0.kill();
            let _ = self.0.try_wait();
        }
    }
}

fn stop_startup(child: &mut std::process::Child) -> Result<std::process::ExitStatus, String> {
    let _ = child.kill();
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|e| format!("Cannot stop startup: {e}"))?
        {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "The OS has not released unresponsive startup process {}; check the graphics/camera driver before restarting",
                child.id()
            ));
        }
        thread::sleep(Duration::from_millis(10));
    }
}
impl Outcome {
    fn retry(&self) -> bool {
        !self.success
            && !self.ready
            && (self.timed_out
                || self.graphics_error
                || matches!(self.signal, Some(6 | 7 | 11))
                || matches!(self.code, Some(134 | 135 | 139)))
    }
}

fn attempt(
    mut command: Command,
    mode: Mode,
    log_file: Log,
    timeout: Duration,
) -> Result<Outcome, String> {
    configure(&mut command, mode);
    command
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut owned = OwnedChild(
        command
            .spawn()
            .map_err(|e| format!("Cannot start the native window: {e}"))?,
    );
    let child = &mut owned.0;
    let observation = Observation::default();
    let mut readers = Vec::new();
    if let Some(output) = child.stdout.take() {
        let log = log_file.clone();
        let state = observation.clone();
        readers.push(
            thread::Builder::new()
                .name("hejjetrac-stdout-log".into())
                .spawn(move || pump(output, log, state, false))
                .map_err(|e| format!("Cannot start diagnostics reader: {e}"))?,
        );
    }
    if let Some(output) = child.stderr.take() {
        let log = log_file.clone();
        let state = observation.clone();
        readers.push(
            thread::Builder::new()
                .name("hejjetrac-stderr-log".into())
                .spawn(move || pump(output, log, state, true))
                .map_err(|e| format!("Cannot start diagnostics reader: {e}"))?,
        );
    }
    let start = Instant::now();
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|e| format!("Cannot observe native startup: {e}"))?
        {
            break status;
        }
        if observation.ready.load(Ordering::Acquire) {
            break child
                .wait()
                .map_err(|e| format!("Cannot wait for the native window: {e}"))?;
        }
        if start.elapsed() >= timeout {
            timed_out = true;
            break stop_startup(child)?;
        }
        thread::sleep(Duration::from_millis(25));
    };
    let drain_deadline = Instant::now() + Duration::from_secs(1);
    for reader in readers {
        while !reader.is_finished() && Instant::now() < drain_deadline {
            thread::sleep(Duration::from_millis(10));
        }
        if reader.is_finished() {
            let _ = reader.join();
        } else {
            log(
                &log_file,
                "Diagnostic stream did not close within the bounded drain interval.\n",
            );
        }
    }
    let outcome = Outcome {
        success: status.success(),
        signal: status.signal(),
        code: status.code(),
        ready: observation.ready.load(Ordering::Acquire),
        graphics_error: observation.graphics_error.load(Ordering::Acquire),
        timed_out,
    };
    log(
        &log_file,
        &format!(
            "\nAttempt {mode:?}: exit={:?}, signal={:?}, ready={}, timeout={}\n",
            outcome.code, outcome.signal, outcome.ready, outcome.timed_out
        ),
    );
    Ok(outcome)
}

/// None means this is the actual GUI/utility process, not its startup supervisor.
pub fn supervise() -> Result<Option<ExitCode>, String> {
    if std::env::var_os(CHILD).is_some() {
        return Ok(None);
    }
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.iter().any(|s| {
        [
            "--help",
            "-h",
            "--version",
            "-V",
            "--camera-list",
            "--probe-camera",
            "--diagnostics",
        ]
        .iter()
        .any(|flag| s == flag)
    }) {
        return Ok(None);
    }
    let safe = args.iter().any(|s| s == "--safe-mode");
    let executable =
        std::env::current_exe().map_err(|e| format!("Cannot locate this application: {e}"))?;
    if args.iter().any(|s| s == "--control-stdio") {
        if safe {
            use std::os::unix::process::CommandExt;
            let mut command = Command::new(executable);
            command.args(args);
            configure(&mut command, Mode::Software);
            return Err(format!(
                "Cannot start software graphics for the native bridge: {}",
                command.exec()
            ));
        }
        return Ok(None);
    }
    let (path, log_file) = match open_log(&directory()) {
        Ok(pair) => (Some(pair.0), pair.1),
        Err(error) => {
            eprintln!("{error}; startup will continue without a log file");
            (None, None)
        }
    };
    log(&log_file, &environment_report());
    if let Some(path) = &path {
        eprintln!("Hejjetrac diagnostics: {}", path.display());
    }
    let explicit_gpu = std::env::var_os("WGPU_BACKEND").is_some()
        || std::env::var_os("WGPU_ADAPTER_NAME").is_some();
    for mode in modes(safe, explicit_gpu) {
        log(&log_file, &format!("\nStarting graphics mode {mode:?}\n"));
        let mut command = Command::new(&executable);
        command.args(&args);
        let result = match attempt(command, mode, log_file.clone(), STARTUP_TIMEOUT) {
            Ok(result) => result,
            Err(error) => {
                log(&log_file, &format!("{error}\n"));
                let location = path
                    .as_ref()
                    .map_or_else(|| "stderr".into(), |p| p.display().to_string());
                return Err(format!("{error}. Diagnostics: {location}"));
            }
        };
        if result.success {
            return Ok(Some(ExitCode::SUCCESS));
        }
        if !result.retry() {
            break;
        }
        eprintln!(
            "Hejjetrac graphics startup failed in {mode:?}; trying the next compatible startup mode."
        );
    }
    let location = path.map_or_else(
        || "stderr (the log directory was unavailable)".into(),
        |p| p.display().to_string(),
    );
    let message = format!(
        "The native window could not continue. Diagnostics: {location}. Try --safe-mode for software graphics and --diagnostics for system details. Automatic retries stop after a window has rendered; an active tracking session is never silently restarted."
    );
    log(&log_file, &format!("{message}\n"));
    Err(message)
}

#[cfg(test)]
mod tests {
    use super::*;
    type TestResult = Result<(), Box<dyn std::error::Error>>;
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Result<Self, Box<dyn std::error::Error>> {
            let path = std::env::temp_dir().join(format!(
                "hejjetrac-launch-test-{}-{}",
                std::process::id(),
                SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
            ));
            fs::create_dir(&path)?;
            Ok(Self(path))
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn fixture(case: &str) -> Result<Command, Box<dyn std::error::Error>> {
        let mut command = Command::new(std::env::current_exe()?);
        command
            .args(["--exact", "launch::tests::fixture_child", "--nocapture"])
            .env("HEJJETRAC_LAUNCH_TEST_CASE", case);
        Ok(command)
    }
    #[test]
    fn fixture_child() -> TestResult {
        let Ok(case) = std::env::var("HEJJETRAC_LAUNCH_TEST_CASE") else {
            return Ok(());
        };
        match case.as_str() {
            "early" => {
                eprintln!("WGPU error: simulated startup adapter failure");
                std::process::exit(1);
            }
            "late" => {
                std::io::stderr().write_all(b"Hejjetrac desk")?;
                thread::sleep(Duration::from_millis(10));
                std::io::stderr()
                    .write_all(b"top ready\nWGPU error: simulated running failure\n")?;
                std::process::exit(1);
            }
            "hang" => loop {
                thread::park_timeout(Duration::from_secs(1));
            },
            _ => {}
        }
        Ok(())
    }
    #[test]
    fn real_child_pipes_record_both_streams_and_never_retry_a_running_failure() -> TestResult {
        let temp = Temp::new()?;
        let (path, journal) = open_log(&temp.0)?;
        let early = attempt(
            fixture("early")?,
            Mode::Automatic,
            journal.clone(),
            Duration::from_secs(2),
        )?;
        assert!(!early.ready && early.retry());
        let late = attempt(
            fixture("late")?,
            Mode::Automatic,
            journal,
            Duration::from_secs(2),
        )?;
        assert!(late.ready && !late.retry());
        let text = fs::read_to_string(path)?;
        assert!(text.contains("running 1 test"));
        assert!(text.contains("simulated startup adapter failure"));
        assert!(text.contains(READY));
        Ok(())
    }
    #[test]
    fn hung_startup_is_killed_and_reaped_within_a_bound() -> TestResult {
        let tick = Instant::now();
        let outcome = attempt(
            fixture("hang")?,
            Mode::Automatic,
            None,
            Duration::from_millis(150),
        )?;
        assert!(outcome.timed_out && !outcome.ready && outcome.retry());
        assert!(tick.elapsed() < Duration::from_secs(3));
        Ok(())
    }
    #[test]
    fn capped_driver_output_retains_the_final_error_summary() -> TestResult {
        let temp = Temp::new()?;
        let (path, journal) = open_log(&temp.0)?;
        let arc = journal.as_ref().ok_or("missing log")?;
        {
            let mut writer = arc.lock().map_err(|_| "log lock failed")?;
            writer.limit = 8;
            writer.write(b"01234567890123456789");
            writer.write(b"discarded");
        }
        log(&journal, "fatal startup summary\n");
        let bytes = fs::read(path)?;
        assert_eq!(&bytes[..8], b"01234567");
        assert!(String::from_utf8_lossy(&bytes).ends_with("fatal startup summary\n"));
        Ok(())
    }
    #[test]
    fn defaults_are_bounded_and_explicit_gpu_choices_are_preserved() {
        assert_eq!(
            modes(false, false),
            vec![Mode::Automatic, Mode::Vulkan, Mode::Software]
        );
        assert_eq!(modes(false, true), vec![Mode::Automatic]);
        assert_eq!(modes(true, true), vec![Mode::Software]);
    }
    #[test]
    fn only_pre_window_graphics_crashes_or_timeouts_retry() {
        let mut o = Outcome {
            success: false,
            signal: Some(11),
            code: None,
            ready: false,
            graphics_error: false,
            timed_out: false,
        };
        assert!(o.retry());
        o.ready = true;
        assert!(!o.retry());
        o.ready = false;
        o.signal = None;
        o.code = Some(1);
        assert!(!o.retry());
        o.graphics_error = true;
        assert!(o.retry());
        o.ready = true;
        assert!(!o.retry());
        o.ready = false;
        o.graphics_error = false;
        o.timed_out = true;
        assert!(o.retry());
    }
    #[test]
    fn ready_and_graphics_markers_survive_split_reads() {
        let o = Observation::default();
        o.inspect("prefix Hejjetrac desktop ready\n");
        assert!(o.ready.load(Ordering::Acquire));
        o.inspect("WGPU error: no adapter");
        assert!(o.graphics_error.load(Ordering::Acquire));
    }
    #[test]
    fn state_paths_respect_absolute_xdg_and_do_not_require_home() {
        assert_eq!(
            state_directory(|key| (key == "XDG_STATE_HOME").then(|| PathBuf::from("/state"))),
            PathBuf::from("/state/hejjetrac/logs")
        );
        assert_eq!(
            state_directory(|key| match key {
                "XDG_STATE_HOME" => Some("relative".into()),
                "HOME" => Some("/user".into()),
                _ => None,
            }),
            PathBuf::from("/user/.local/state/hejjetrac/logs")
        );
        assert!(state_directory(|_| None).is_absolute());
    }
}
