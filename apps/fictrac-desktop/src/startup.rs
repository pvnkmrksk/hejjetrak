//! A writable, persistent default without changing installed application resources.
use fictrac_io::{ConfigDocument, SourceSpec};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

const MAX_CONFIG_BYTES: u64 = 1_000_000;
static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

pub struct Startup {
    pub document: ConfigDocument,
    pub path: PathBuf,
    pub fresh: bool,
    pub temporary: bool,
    pub notice: String,
}

pub fn default_document(template: &Path) -> Result<Startup, String> {
    default_at(template, user_config_path())
}

fn default_at(template: &Path, destination: Result<PathBuf, String>) -> Result<Startup, String> {
    match destination.and_then(|destination| initialize(template, &destination)) {
        Ok(mut startup) => {
            // Open-four discovers sibling rig files beside the saved default.
            // Seed each independently; existing calibrations are never replaced
            // and one failed companion must not discard a working VR1.
            if template.file_name().is_some_and(|name| name == "VR1.txt") {
                let mut failures = Vec::new();
                for rig in 2..=4 {
                    let name = format!("VR{rig}.txt");
                    if let Err(error) = initialize(
                        &template.with_file_name(&name),
                        &startup.path.with_file_name(&name),
                    ) {
                        failures.push(format!("{name}: {error}"));
                    }
                }
                if !failures.is_empty() {
                    startup.notice = format!(
                        "Some companion rig configurations are unavailable: {}. VR1 remains available; check the saved rig files beside {} before using Open four.",
                        failures.join("; "),
                        startup.path.display()
                    );
                }
            }
            Ok(startup)
        }
        Err(reason) => Ok(Startup {
            document: read_document(template)?,
            path: template.to_owned(),
            fresh: false,
            temporary: true,
            notice: format!(
                "Cannot use the saved default configuration: {reason}. Using the bundled preview; use Save As to keep your calibration in a writable folder."
            ),
        }),
    }
}

fn user_config_path() -> Result<PathBuf, String> {
    config_path(std::env::consts::OS, |name| {
        std::env::var_os(name).map(PathBuf::from)
    })
}

fn config_path(platform: &str, env: impl Fn(&str) -> Option<PathBuf>) -> Result<PathBuf, String> {
    let absolute_env = |name| env(name).filter(|path| path.is_absolute());
    let directory = match platform {
        "macos" => {
            absolute_env("HOME").map(|home| home.join("Library/Application Support/Hejjetrac"))
        }
        "windows" => absolute_env("APPDATA").map(|data| data.join("Hejjetrac")),
        _ => absolute_env("XDG_CONFIG_HOME")
            .map(|config| config.join("hejjetrac"))
            .or_else(|| absolute_env("HOME").map(|home| home.join(".config/hejjetrac"))),
    }
    .ok_or("The per-user configuration directory is unavailable")?;
    Ok(directory.join("VR1.txt"))
}

fn read_document(path: &Path) -> Result<ConfigDocument, String> {
    let mut text = String::new();
    File::open(path)
        .and_then(|file| file.take(MAX_CONFIG_BYTES + 1).read_to_string(&mut text))
        .map_err(|error| format!("Cannot read {}: {error}", path.display()))?;
    ConfigDocument::parse(text).map_err(|error| format!("{}: {error}", path.display()))
}

fn initialize(template: &Path, destination: &Path) -> Result<Startup, String> {
    match File::open(destination) {
        Ok(_) => return existing(destination),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("Cannot open {}: {error}", destination.display())),
    }
    let mut document = read_document(template)?;
    if let SourceSpec::Video(source) = document
        .source(Some(template))
        .map_err(|error| error.to_string())?
    {
        // Moving the config must not move its source, including a currently missing video.
        let absolute = fs::canonicalize(&source)
            .or_else(|_| std::path::absolute(&source))
            .map_err(|error| format!("Cannot resolve video {}: {error}", source.display()))?;
        document
            .set(
                "src_fn",
                absolute
                    .to_str()
                    .ok_or("The video path cannot be stored as UTF-8 configuration text")?,
            )
            .map_err(|error| error.to_string())?;
    }
    let parent = destination
        .parent()
        .ok_or("The default configuration has no parent directory")?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Cannot create {}: {error}", parent.display()))?;
    if !publish_new(parent, destination, document.text().as_bytes())? {
        return existing(destination);
    }
    Ok(Startup {
        document,
        path: destination.to_owned(),
        fresh: true,
        temporary: false,
        notice: String::new(),
    })
}

fn existing(path: &Path) -> Result<Startup, String> {
    Ok(Startup {
        document: read_document(path)?,
        path: path.to_owned(),
        fresh: false,
        temporary: false,
        notice: String::new(),
    })
}

struct TemporaryFile(PathBuf);
impl Drop for TemporaryFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn publish_new(parent: &Path, destination: &Path, bytes: &[u8]) -> Result<bool, String> {
    // Both create_new and hard_link are no-clobber operations. A concurrent launch
    // can read only the complete, synced file, never a partly written default.
    let mut temporary = None;
    for _ in 0..64 {
        let path = parent.join(format!(
            ".hejjetrac-startup-{}-{}.tmp",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => {
                temporary = Some((TemporaryFile(path), file));
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(format!("Cannot create {}: {error}", path.display())),
        }
    }
    let (temporary, mut file) = temporary.ok_or("Cannot allocate a startup configuration file")?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("Cannot write {}: {error}", temporary.0.display()))?;
    drop(file);
    match fs::hard_link(&temporary.0, destination) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        Err(error) => Err(format!(
            "Cannot create {} atomically: {error}",
            destination.display()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Result<Self, std::io::Error> {
            let path = std::env::temp_dir().join(format!(
                "hejjetrac-startup-test-{}-{}",
                std::process::id(),
                NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path)?;
            Ok(Self(path))
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn first_copy_preserves_bundle_and_later_loads_preserve_user_source() -> TestResult {
        let directory = Directory::new()?;
        let template = directory.0.join("bundled.txt");
        let destination = directory.0.join("user/VR1.txt");
        let original = "# Original spacing\r\nsrc_fn  : 0 # camera\r\nunknown : preserve me\r\n";
        fs::write(&template, original)?;
        let first = initialize(&template, &destination)?;
        assert!(first.fresh && !first.temporary);
        assert_eq!(first.document.text(), original);
        assert_eq!(fs::read_to_string(&template)?, original);
        let saved = "# Calibrated\nsrc_fn : 7\nvfov : 21\n";
        fs::write(&destination, saved)?;
        fs::write(&template, "invalid new bundle")?;
        let later = initialize(&template, &destination)?;
        assert!(!later.fresh && !later.temporary);
        assert_eq!(later.document.text(), saved);
        assert_eq!(
            later.document.source(Some(&later.path))?,
            SourceSpec::Camera(7)
        );
        assert_eq!(fs::read_to_string(destination)?, saved);
        Ok(())
    }

    #[test]
    fn default_seeds_companion_rigs_and_preserves_saved_calibrations() -> TestResult {
        let directory = Directory::new()?;
        let bundle = directory.0.join("bundle");
        let user = directory.0.join("user");
        fs::create_dir(&bundle)?;
        for rig in 1..=4 {
            fs::write(
                bundle.join(format!("VR{rig}.txt")),
                format!(
                    "# Bundled rig {rig}\nsrc_fn : {}\nsock_port : {}\n",
                    (rig - 1) * 2,
                    1110 + rig
                ),
            )?;
        }
        let template = bundle.join("VR1.txt");
        let destination = user.join("VR1.txt");
        let first = default_at(&template, Ok(destination.clone()))?;
        assert!(first.fresh && !first.temporary);
        assert!(first.notice.is_empty());
        for rig in 1..=4 {
            let name = format!("VR{rig}.txt");
            assert_eq!(
                fs::read_to_string(user.join(&name))?,
                fs::read_to_string(bundle.join(&name))?
            );
        }
        let calibrated =
            "# User calibration\nsrc_fn : 12\nroi_r : 0.3\nsock_port : 5555\nzmq_port : 6666\n";
        fs::write(user.join("VR2.txt"), calibrated)?;
        fs::write(bundle.join("VR2.txt"), "invalid new bundle")?;
        fs::remove_file(user.join("VR4.txt"))?;
        let later = default_at(&template, Ok(destination.clone()))?;
        assert!(!later.fresh && !later.temporary);
        assert!(later.notice.is_empty());
        assert_eq!(later.path, destination);
        assert_eq!(fs::read_to_string(user.join("VR2.txt"))?, calibrated);
        assert_eq!(
            fs::read_to_string(user.join("VR4.txt"))?,
            fs::read_to_string(bundle.join("VR4.txt"))?
        );
        assert_eq!(fs::read_dir(&user)?.count(), 4);
        Ok(())
    }

    #[test]
    fn failed_companion_preserves_working_default_and_other_rigs() -> TestResult {
        let directory = Directory::new()?;
        let bundle = directory.0.join("bundle");
        let user = directory.0.join("user");
        fs::create_dir(&bundle)?;
        fs::create_dir(&user)?;
        for rig in 1..=4 {
            fs::write(
                bundle.join(format!("VR{rig}.txt")),
                format!("src_fn : {rig}\n"),
            )?;
        }
        // An existing invalid user file remains intact and the next rig is
        // still seeded. This must not fall back to the installed VR1 template.
        let invalid = "retain this invalid user configuration";
        fs::write(user.join("VR3.txt"), invalid)?;
        let destination = user.join("VR1.txt");
        let startup = default_at(&bundle.join("VR1.txt"), Ok(destination.clone()))?;
        assert!(startup.fresh && !startup.temporary);
        assert_eq!(startup.path, destination);
        assert_eq!(startup.document.text(), "src_fn : 1\n");
        assert_eq!(fs::read_to_string(user.join("VR3.txt"))?, invalid);
        assert_eq!(fs::read_to_string(user.join("VR4.txt"))?, "src_fn : 4\n");
        assert!(startup.notice.contains("VR3.txt"));
        assert!(startup.notice.contains("VR1 remains available"));
        assert!(startup.notice.contains("before using Open four"));
        assert_eq!(fs::read_dir(&user)?.count(), 4);
        Ok(())
    }

    #[test]
    fn moved_default_keeps_its_video_and_all_other_text() -> TestResult {
        let directory = Directory::new()?;
        let template = directory.0.join("bundle/default.txt");
        let source = directory.0.join("bundle/video.avi");
        fs::create_dir_all(template.parent().ok_or("missing parent")?)?;
        fs::write(&source, b"video fixture")?;
        let original = "# Keep comments\r\nsrc_fn : video.avi # source\r\nunknown : unchanged\r\n";
        fs::write(&template, original)?;
        let destination = directory.0.join("user/VR1.txt");
        let startup = initialize(&template, &destination)?;
        let absolute = fs::canonicalize(source)?;
        assert_eq!(
            startup.document.source(Some(&startup.path))?,
            SourceSpec::Video(absolute.clone())
        );
        assert_eq!(
            startup.document.text(),
            original.replace("video.avi", &absolute.to_string_lossy())
        );
        assert_eq!(fs::read_to_string(template)?, original);
        Ok(())
    }

    #[test]
    fn concurrent_first_launches_publish_one_complete_default() -> TestResult {
        let directory = Directory::new()?;
        let template = directory.0.join("bundle.txt");
        let destination = directory.0.join("user/VR1.txt");
        let original = format!("# {}\nsrc_fn : 3\n", "x".repeat(100_000));
        fs::write(&template, &original)?;
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
        let mut threads = Vec::new();
        for _ in 0..8 {
            let (template, destination, barrier) =
                (template.clone(), destination.clone(), barrier.clone());
            threads.push(std::thread::spawn(move || {
                barrier.wait();
                initialize(&template, &destination)
            }));
        }
        let mut creators = 0;
        for thread in threads {
            let startup = thread.join().map_err(|_| "startup thread failed")??;
            creators += usize::from(startup.fresh);
            assert_eq!(startup.document.text(), original);
        }
        assert_eq!(creators, 1);
        assert_eq!(fs::read_to_string(&destination)?, original);
        assert_eq!(
            fs::read_dir(destination.parent().ok_or("missing parent")?)?.count(),
            1
        );
        Ok(())
    }

    #[test]
    fn oversized_or_invalid_existing_configs_are_never_replaced() -> TestResult {
        let directory = Directory::new()?;
        let template = directory.0.join("bundle.txt");
        let destination = directory.0.join("VR1.txt");
        fs::write(&template, "src_fn : 0\n")?;
        for original in [
            "invalid".to_owned(),
            "x".repeat(MAX_CONFIG_BYTES as usize + 1),
        ] {
            fs::write(&destination, &original)?;
            assert!(initialize(&template, &destination).is_err());
            assert_eq!(fs::read_to_string(&destination)?, original);
        }
        Ok(())
    }

    #[test]
    fn unwritable_default_keeps_bundled_preview_and_save_as_guidance() -> TestResult {
        let directory = Directory::new()?;
        let template = directory.0.join("bundle.txt");
        let blocked = directory.0.join("not-a-directory");
        let original = "src_fn : 0\n# Original template\n";
        fs::write(&template, original)?;
        fs::write(&blocked, b"preserve this file")?;
        let startup = default_at(&template, Ok(blocked.join("VR1.txt")))?;
        assert!(startup.temporary);
        assert!(!startup.fresh);
        assert_eq!(startup.path, template);
        assert_eq!(startup.document.text(), original);
        assert!(startup.notice.contains("Save As"));
        assert_eq!(fs::read_to_string(&blocked)?, "preserve this file");
        assert_eq!(fs::read_to_string(&startup.path)?, original);
        Ok(())
    }

    #[test]
    fn native_config_locations_and_relative_xdg_fallback() -> TestResult {
        let directory = Directory::new()?;
        let env = |name: &str| match name {
            "HOME" | "APPDATA" => Some(directory.0.clone()),
            "XDG_CONFIG_HOME" => Some(PathBuf::from("relative-config")),
            _ => None,
        };
        assert_eq!(
            config_path("macos", env)?,
            directory
                .0
                .join("Library/Application Support/Hejjetrac/VR1.txt")
        );
        assert_eq!(
            config_path("windows", env)?,
            directory.0.join("Hejjetrac/VR1.txt")
        );
        assert_eq!(
            config_path("linux", env)?,
            directory.0.join(".config/hejjetrac/VR1.txt")
        );
        assert_eq!(
            config_path("linux", |_| Some(directory.0.clone()))?,
            directory.0.join("hejjetrac/VR1.txt")
        );
        assert!(config_path("linux", |_| None).is_err());
        Ok(())
    }
}
