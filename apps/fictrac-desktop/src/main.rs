#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
mod calibration;
mod control;
mod runtime;
mod session;
mod ui;
use clap::Parser;
use eframe::egui;
use fictrac_io::ConfigDocument;
use session::Session;
use std::{path::PathBuf, time::Instant};
#[derive(Parser)]
#[command(
    version,
    about = "Native FicTrac configuration, video and camera tracking"
)]
struct Args {
    #[arg(short, long)]
    config: Option<PathBuf>,
    #[arg(long, default_value_t = 1)]
    windows: usize,
    #[arg(long)]
    screenshot: Option<PathBuf>,
    #[arg(long, default_value_t = 2.0)]
    screenshot_after: f64,
    #[arg(long)]
    exit_after: Option<f64>,
    #[arg(long)]
    camera_list: bool,
    #[arg(long)]
    probe_camera: Option<u32>,
    #[arg(long)]
    state_output: Option<PathBuf>,
    /// Accept agent JSONL commands on stdin and return structured responses on stdout.
    #[arg(long)]
    control_stdio: bool,
}
fn default_config() -> Result<PathBuf, String> {
    let mut roots = vec![];
    if let Ok(exe) = std::env::current_exe()
        && let Some(parent) = exe.parent()
    {
        roots.push(parent.to_owned());
        roots.push(parent.join("../Resources"));
    }
    if let Ok(cwd) = std::env::current_dir() {
        roots.push(cwd);
    }
    roots.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."));
    roots
        .into_iter()
        .map(|p| p.join("examples/upstream-config.txt"))
        .find(|p| p.is_file())
        .ok_or_else(|| {
            "Cannot find bundled examples/upstream-config.txt; select a config with --config PATH"
                .into()
        })
}
struct Desktop {
    sessions: Vec<Session>,
    next_id: u64,
    args: Args,
    start: Instant,
    requested_screenshot: bool,
    last_state: Instant,
    control: Option<control::Bridge>,
}
impl Desktop {
    fn new(cc: &eframe::CreationContext<'_>, args: Args) -> Result<Self, String> {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        let path = args.config.clone().map(Ok).unwrap_or_else(default_config)?;
        let doc = ConfigDocument::parse(std::fs::read_to_string(&path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        let mut first = Session::new(1, doc, Some(path));
        let result = first.start();
        first.report(result);
        if args.control_stdio {
            first.paused = true;
            first.send(runtime::Command::Pause(true));
        }
        let control = if args.control_stdio {
            Some(control::Bridge::start()?)
        } else {
            None
        };
        let mut app = Self {
            sessions: vec![first],
            next_id: 2,
            args,
            start: Instant::now(),
            requested_screenshot: false,
            last_state: Instant::now() - std::time::Duration::from_secs(1),
            control,
        };
        for _ in 1..app.args.windows.clamp(1, 16) {
            app.spawn_from(app.sessions.len() - 1);
        }
        Ok(app)
    }
    fn spawn_from(&mut self, index: usize) {
        if self.sessions.len() >= 16 {
            self.sessions[index].error = "At most 16 active instances".into();
            return;
        }
        let result = self.sessions[index].document().and_then(|mut doc| {
            let mut path = self.sessions[index].path.clone();
            for _ in 0..16 {
                let (next, next_path) = doc
                    .next_instance(path.as_deref())
                    .map_err(|e| e.to_string())?;
                let used_path = next_path
                    .as_ref()
                    .is_some_and(|p| self.sessions.iter().any(|s| s.path.as_ref() == Some(p)));
                let camera = next.get("src_fn").and_then(|v| v.parse::<u32>().ok());
                let used_camera = camera.is_some_and(|id| {
                    self.sessions.iter().any(|s| {
                        s.document()
                            .ok()
                            .and_then(|d| d.get("src_fn").and_then(|v| v.parse::<u32>().ok()))
                            == Some(id)
                    })
                });
                if !used_path && !used_camera {
                    return Ok((next, next_path));
                }
                doc = next;
                path = next_path;
            }
            Err("No free instance/config slot in the next 16 sources".into())
        });
        match result {
            Ok((doc, path)) => {
                let anchor = self.sessions[index]
                    .path
                    .clone()
                    .or(self.sessions[index].source_base.clone());
                let mut next = Session::new(self.next_id, doc, path);
                if next.path.is_none() {
                    next.source_base = anchor;
                }
                self.next_id += 1;
                let result = next.start();
                next.report(result);
                self.sessions.push(next);
            }
            Err(e) => self.sessions[index].error = e,
        }
    }
}
impl eframe::App for Desktop {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(mut bridge) = self.control.take() {
            bridge.poll(self);
            self.control = Some(bridge);
        }
        self.sessions[0].show(ctx);
        for session in self.sessions.iter_mut().skip(1) {
            let id = egui::ViewportId::from_hash_of(("fictrac", session.id));
            ctx.show_viewport_immediate(
                id,
                egui::ViewportBuilder::default()
                    .with_title(format!("FicTrac · instance {}", session.id))
                    .with_inner_size([1420.0, 940.0])
                    .with_min_inner_size([1000.0, 700.0]),
                |ctx, class| {
                    session.native_window = class != egui::ViewportClass::Embedded;
                    session.show(ctx);
                },
            );
        }
        let requests: Vec<_> = self
            .sessions
            .iter_mut()
            .enumerate()
            .filter_map(|(i, s)| {
                let n = s.spawn_request;
                s.spawn_request = 0;
                (n > 0).then_some((i, n))
            })
            .collect();
        for (mut i, requested) in requests {
            let n = if requested == 3 {
                4usize.saturating_sub(self.sessions.len())
            } else {
                requested
            };
            for _ in 0..n {
                self.spawn_from(i);
                i = self.sessions.len() - 1;
            }
        }
        if self.sessions[0].closed {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        let root = self.sessions[0].id;
        self.sessions.retain(|s| s.id == root || !s.closed);
        let elapsed = self.start.elapsed().as_secs_f64();
        if self.args.screenshot.is_some()
            && !self.requested_screenshot
            && elapsed > self.args.screenshot_after
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            self.requested_screenshot = true;
        }
        if let Some(path) = &self.args.screenshot {
            let screenshots: Vec<_> = ctx.input(|i| {
                i.events
                    .iter()
                    .filter_map(|e| {
                        if let egui::Event::Screenshot { image, .. } = e {
                            Some(image.clone())
                        } else {
                            None
                        }
                    })
                    .collect()
            });
            for screenshot in screenshots {
                let bytes: Vec<_> = screenshot
                    .pixels
                    .iter()
                    .flat_map(|c| c.to_array())
                    .collect();
                let result = image::save_buffer(
                    path,
                    &bytes,
                    screenshot.width() as u32,
                    screenshot.height() as u32,
                    image::ColorType::Rgba8,
                );
                if let Err(e) = result {
                    self.sessions[0].error = format!("screenshot: {e}");
                }
            }
        }
        if self.last_state.elapsed().as_millis() > 400
            && let Some(path) = &self.args.state_output
        {
            let instances:Vec<_>=self.sessions.iter().map(|s|serde_json::json!({"id":s.id,"frames":s.snapshot.frames,"source":s.source_edit,"config_path":s.path,"paused":s.snapshot.paused,"native_window":s.native_window,"revision":s.snapshot.revision,"notice":s.snapshot.notice,"error":s.error,"config_text":s.history.text})).collect();
            let value = serde_json::json!({"instances":instances,"elapsed_seconds":elapsed});
            match serde_json::to_vec_pretty(&value)
                .map_err(|e| e.to_string())
                .and_then(|data| std::fs::write(path, data).map_err(|e| e.to_string()))
            {
                Ok(()) => {}
                Err(e) => self.sessions[0].error = e,
            }
            self.last_state = Instant::now();
        }
        if self.args.exit_after.is_some_and(|t| elapsed > t) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}
fn run() -> Result<(), String> {
    let args = Args::parse();
    if args.camera_list || args.probe_camera.is_some() {
        for camera in fictrac_camera::list_cameras().map_err(|e| e.to_string())? {
            println!("{}: {}", camera.id, camera.name);
        }
        if let Some(id) = args.probe_camera {
            fictrac_camera::request_permission().map_err(|e| e.to_string())?;
            let mut camera = fictrac_camera::Camera::open(id, 30.0).map_err(|e| e.to_string())?;
            let start = Instant::now();
            let mut frames = 0;
            while frames < 30 && start.elapsed().as_secs() < 10 {
                if let Some(frame) = camera.next_frame().map_err(|e| e.to_string())? {
                    frames += 1;
                    if frames == 1 {
                        println!(
                            "capture {} × {} at {} ms",
                            frame.gray.width(),
                            frame.gray.height(),
                            frame.gray.timestamp_ms()
                        );
                    }
                }
            }
            if frames == 0 {
                return Err("No camera frames received within 10 seconds".into());
            }
            println!(
                "{frames} captured frames in {:.3} seconds",
                start.elapsed().as_secs_f64()
            );
        }
        return Ok(());
    }
    if args.state_output.as_ref().is_some_and(|p| p.exists()) {
        return Err("state output already exists".into());
    }
    if args.screenshot.as_ref().is_some_and(|p| p.exists()) {
        return Err("screenshot output already exists".into());
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("FicTrac · native Rust")
            .with_inner_size([1480.0, 960.0])
            .with_min_inner_size([1000.0, 700.0]),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "FicTrac",
        options,
        Box::new(move |cc| {
            Desktop::new(cc, args)
                .map(|a| Box::new(a) as Box<dyn eframe::App>)
                .map_err(|e| e.into())
        }),
    )
    .map_err(|e| e.to_string())
}
fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("FicTrac: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}
