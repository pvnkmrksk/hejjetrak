#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
mod calibration;
mod control;
mod icons;
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
    name = "hejjetrac",
    version,
    about = "Native spherical motion tracking, compatible with FicTrac"
)]
struct Args {
    /// Preview size in logical pixels, e.g. 480x360.
    #[arg(long, value_parser = parse_size)]
    window_size: Option<Pair>,
    /// Top-left window location in logical pixels, e.g. 0,24.
    #[arg(long, value_parser = parse_position, allow_hyphen_values = true)]
    window_position: Option<Pair>,
    /// Tile columns x rows, e.g. --windows 16 --grid 4x4.
    #[arg(long, value_parser = parse_grid)]
    grid: Option<Pair>,
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
fn resource(relative: &str) -> Result<PathBuf, String> {
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
        .map(|p| p.join(relative))
        .find(|p| p.is_file())
        .ok_or_else(|| format!("Cannot find bundled {relative}; provide --config PATH"))
}
fn default_config() -> Result<PathBuf, String> {
    resource("VR1.txt").or_else(|_| resource("examples/upstream-config.txt"))
}
fn fallback_document(source: &ConfigDocument) -> Result<ConfigDocument, String> {
    let path = resource("examples/upstream-config.txt")?;
    let mut fallback =
        ConfigDocument::parse(std::fs::read_to_string(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    fallback
        .set(
            "src_fn",
            &resource("examples/sample.mp4")?.to_string_lossy(),
        )
        .map_err(|e| e.to_string())?;
    for (key, value) in source.fields().filter(|(key, _)| {
        key.starts_with("zmq_")
            || key.starts_with("sock_")
            || key.starts_with("com_")
            || matches!(
                *key,
                "rust_instance_name" | "output_fn" | "save_raw" | "save_debug" | "vid_codec"
            )
    }) {
        fallback.set(key, value).map_err(|e| e.to_string())?;
    }
    Ok(fallback)
}
#[derive(Clone, Copy, Debug)]
struct Pair(f32, f32);
fn parse_pair(value: &str, separator: char) -> Result<Pair, String> {
    let (x, y) = value.split_once(separator).ok_or("Expected two numbers")?;
    let pair = Pair(
        x.parse().map_err(|_| "Invalid first coordinate")?,
        y.parse().map_err(|_| "Invalid second coordinate")?,
    );
    if !pair.0.is_finite() || !pair.1.is_finite() {
        return Err("Coordinates must be finite".into());
    }
    Ok(pair)
}
fn parse_size(value: &str) -> Result<Pair, String> {
    let pair = parse_pair(value, 'x')?;
    if !(96.0..=8192.0).contains(&pair.0) || !(96.0..=8192.0).contains(&pair.1) {
        return Err("Window size must be 96x96 through 8192x8192".into());
    }
    Ok(pair)
}
fn parse_position(value: &str) -> Result<Pair, String> {
    let pair = parse_pair(value, ',')?;
    if pair.0.abs() > 1_000_000.0 || pair.1.abs() > 1_000_000.0 {
        return Err("Window position must be within ±1000000 logical pixels".into());
    }
    Ok(pair)
}
fn parse_grid(value: &str) -> Result<Pair, String> {
    let pair = parse_pair(value, 'x')?;
    if pair.0.fract() != 0.0
        || pair.1.fract() != 0.0
        || !(1.0..=16.0).contains(&pair.0)
        || !(1.0..=16.0).contains(&pair.1)
    {
        return Err("Grid dimensions must be integers 1..16".into());
    }
    Ok(pair)
}
struct Desktop {
    sessions: Vec<Session>,
    next_id: u64,
    args: Args,
    start: Instant,
    requested_screenshot: bool,
    last_state: Instant,
    control: Option<control::Bridge>,
    window_size: Pair,
    window_origin: Pair,
    tile_columns: usize,
    root_hidden: bool,
}
impl Desktop {
    fn camera_owners(&self, except: Option<u64>) -> std::collections::BTreeMap<u32, String> {
        self.sessions
            .iter()
            .filter(|session| {
                !session.closed
                    && Some(session.id) != except
                    && session.worker.is_some()
                    && !session.snapshot.ended
            })
            .filter_map(|session| {
                session.running_source.parse::<u32>().ok().map(|id| {
                    (
                        id,
                        format!("{} (window {})", session.display_name(), session.id),
                    )
                })
            })
            .collect()
    }
    fn refresh_camera_occupancy(&mut self) {
        let reservations = self
            .sessions
            .iter()
            .map(|session| (session.id, self.camera_owners(Some(session.id))))
            .collect::<Vec<_>>();
        for session in &mut self.sessions {
            if let Some((_, cameras)) = reservations.iter().find(|(id, _)| *id == session.id) {
                session.occupied_cameras = cameras.clone();
            }
        }
    }
    fn new(cc: &eframe::CreationContext<'_>, args: Args) -> Result<Self, String> {
        if let Some(render_state) = &cc.wgpu_render_state {
            let info = render_state.adapter.get_info();
            eprintln!(
                "Graphics: {} ({:?}, {:?})",
                info.name, info.backend, info.device_type
            );
        }
        ui::configure_appearance(&cc.egui_ctx);
        let path = args.config.clone().map(Ok).unwrap_or_else(|| {
            if args.control_stdio {
                resource("examples/upstream-config.txt")
            } else {
                default_config()
            }
        })?;
        let doc = ConfigDocument::parse(std::fs::read_to_string(&path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        let mut first = Session::new(1, doc.clone(), Some(path.clone()));
        first.framebuffer_limit = cc
            .wgpu_render_state
            .as_ref()
            .map(|state| state.device.limits().max_texture_dimension_2d.min(4096))
            .unwrap_or(4096);
        first.max_window_inner_size =
            ui::framebuffer_window_limit(first.framebuffer_limit, cc.egui_ctx.pixels_per_point());
        cc.egui_ctx
            .send_viewport_cmd(egui::ViewportCommand::MaxInnerSize(
                first.max_window_inner_size,
            ));
        if args.config.is_none() && !args.control_stdio {
            first.startup_fallback = Some(fallback_document(&doc)?);
            if let Some(camera) = first.cameras.first() {
                first.fields(&[("src_fn", camera.id.to_string())])?;
                first.changed = None;
            }
        }
        let result = if first.worker.is_some() {
            Ok(())
        } else {
            first.start()
        };
        if let Err(error) = &result {
            first.use_fallback(error);
        }
        if first.worker.is_none() {
            first.report(result);
        }
        if args.control_stdio {
            first.paused = true;
            first.send(runtime::Command::Pause(true));
        }
        let control = if args.control_stdio {
            Some(control::Bridge::start(cc.egui_ctx.clone())?)
        } else {
            None
        };
        let monitor = cc
            .egui_ctx
            .input(|i| i.viewport().monitor_size)
            .unwrap_or(egui::vec2(1440.0, 900.0));
        let grid = args
            .grid
            .unwrap_or(Pair((monitor.x / 520.0).floor().max(1.0), 1.0));
        let requested_size = args.window_size.unwrap_or_else(|| {
            if args.grid.is_some() {
                Pair(
                    (monitor.x / grid.0 - 8.0).max(96.0),
                    ((monitor.y - 50.0) / grid.1 - 32.0).max(96.0),
                )
            } else {
                Pair(520.0, 480.0)
            }
        });
        let window_size = Pair(
            requested_size.0.min(first.max_window_inner_size.x),
            requested_size.1.min(first.max_window_inner_size.y),
        );
        if window_size.0 != requested_size.0 || window_size.1 != requested_size.1 {
            eprintln!(
                "Window dimensions limited to {}×{} logical pixels to respect the native framebuffer limit",
                window_size.0, window_size.1
            );
        }
        let window_origin = args.window_position.unwrap_or(Pair(12.0, 36.0));
        cc.egui_ctx
            .send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                window_size.0,
                window_size.1,
            )));
        cc.egui_ctx
            .send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(
                window_origin.0,
                window_origin.1,
            )));
        let mut app = Self {
            sessions: vec![first],
            next_id: 2,
            args,
            start: Instant::now(),
            requested_screenshot: false,
            last_state: Instant::now() - std::time::Duration::from_secs(1),
            control,
            window_size,
            window_origin,
            tile_columns: grid.0 as usize,
            root_hidden: false,
        };
        for _ in 1..app.args.windows.clamp(1, 16) {
            app.spawn_from(app.sessions.len() - 1);
        }
        Ok(app)
    }
    fn spawn_from(&mut self, index: usize) {
        if self
            .sessions
            .iter()
            .filter(|session| !session.closed)
            .count()
            >= 16
        {
            self.sessions[index].error = "At most 16 active instances".into();
            return;
        }
        let document = if !self.sessions[index].fallback_notice.is_empty()
            && let Some(path) = &self.sessions[index].path
        {
            std::fs::read_to_string(path)
                .map_err(|e| e.to_string())
                .and_then(|text| ConfigDocument::parse(text).map_err(|e| e.to_string()))
        } else {
            self.sessions[index].document()
        };
        let result = document.and_then(|mut doc| {
            let mut path = self.sessions[index].path.clone();
            for _ in 0..16 {
                let (next, next_path) = doc
                    .next_instance(path.as_deref())
                    .map_err(|e| e.to_string())?;
                let used_path = next_path.as_ref().is_some_and(|p| {
                    self.sessions
                        .iter()
                        .any(|s| !s.closed && s.path.as_ref() == Some(p))
                });
                let camera = next.get("src_fn").and_then(|v| v.parse::<u32>().ok());
                let used_camera = camera.is_some_and(|id| {
                    self.sessions.iter().filter(|s| !s.closed).any(|s| {
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
                let fallback = if self.args.config.is_none()
                    && !self.args.control_stdio
                    && doc.get("src_fn").is_some_and(|v| v.parse::<u32>().is_ok())
                {
                    fallback_document(&doc).ok()
                } else {
                    None
                };
                let mut next = Session::new(self.next_id, doc, path);
                next.framebuffer_limit = self.sessions[index].framebuffer_limit;
                next.max_window_inner_size = self.sessions[index].max_window_inner_size;
                next.startup_fallback = fallback;
                if next.path.is_none() {
                    next.source_base = anchor;
                }
                self.next_id += 1;
                let result = next.start();
                if let Err(error) = &result {
                    next.use_fallback(error);
                }
                if next.worker.is_none() {
                    next.report(result);
                }
                self.sessions.push(next);
            }
            Err(e) => self.sessions[index].error = e,
        }
    }

    fn rig_manager(&mut self, ctx: &egui::Context) {
        if !self
            .sessions
            .iter()
            .any(|session| session.show_rigs && !session.closed)
        {
            return;
        }
        let mut close = false;
        let mut start_all = false;
        let mut open_four = false;
        let mut tile = false;
        let framebuffer_limit = self
            .sessions
            .first()
            .map(|s| s.framebuffer_limit)
            .unwrap_or(4096);
        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("rig-manager"),
            egui::ViewportBuilder::default()
                .with_title("HejjéTrac · Rig manager")
                .with_inner_size([580.0, 540.0])
                .with_max_inner_size(ui::framebuffer_window_limit(framebuffer_limit, ctx.pixels_per_point()))
                .with_min_inner_size([400.0, 300.0]),
            |ctx, _| {
                ui::constrain_framebuffer(ctx, framebuffer_limit);
                close = ctx.input(|i| i.viewport().close_requested());
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.heading("Your rigs");
                    ui.label("Each window has its own source, calibration and outputs.");
                    ui.horizontal_wrapped(|ui| {
                        start_all = ui.button("Start all").on_hover_text("Resume paused sessions, replay finished videos and retry stopped sources. Finish active calibration first.").clicked();
                        open_four = ui.button("Open four rigs").on_hover_text("Open missing windows using the next saved VR/BR configuration, up to four active rigs.").clicked();
                        tile = ui.button("Arrange in a grid").clicked();
                    });
                    ui.separator();
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for session in self.sessions.iter_mut().filter(|session| !session.closed) {
                            ui.group(|ui| {
                                ui.horizontal(|ui| {
                                    ui.strong(session.display_name());
                                    ui.label(if session.tool != session::Tool::Inspect { "Calibrating" } else if session.snapshot.ended { "Finished" } else if session.paused { "Paused" } else { "Running" });
                                    if ui.button("Settings…").clicked() {
                                        session.show_settings = true;
                                        ui.ctx().send_viewport_cmd_to(
                                            egui::ViewportId::from_hash_of(("settings", session.id)),
                                            egui::ViewportCommand::Focus,
                                        );
                                    }
                                });
                                ui.small(session.path.as_ref().map(|path| path.display().to_string()).unwrap_or_else(|| "Unsaved configuration · use Save As to keep this rig".into()));
                                ui.small(format!("{} · {:.1} fps · {:.2} ms tracking", session.running_source, session.snapshot.actual_fps, session.snapshot.tracking_ms));
                                if !session.fallback_notice.is_empty() {
                                    ui.small(&session.fallback_notice);
                                }
                                ui.collapsing("Full config", |ui| {
                                    let mut text = session.history.text.clone();
                                    ui.add(egui::TextEdit::multiline(&mut text).code_editor().desired_rows(10).desired_width(f32::INFINITY).interactive(false));
                                    ui.small("Edit and save this configuration from its Settings window.");
                                });
                                if !session.error.is_empty() {
                                    ui.colored_label(ui.visuals().error_fg_color, &session.error);
                                }
                            });
                        }
                        ui.add_space(8.0);
                        ui.small("Launch a saved grid from Terminal:");
                        ui.code("hejjetrac --config VR1.txt --windows 4 --grid 2x2");
                    });
                });
            },
        );
        if close {
            for session in &mut self.sessions {
                session.show_rigs = false;
            }
        }
        if start_all {
            for session in self.sessions.iter_mut().filter(|session| !session.closed) {
                if session.tool != session::Tool::Inspect {
                    session.error =
                        "Finish or cancel this rig's calibration before Start all".into();
                } else if session.worker.is_none() || session.snapshot.ended {
                    let result = session.start();
                    session.report(result);
                } else {
                    session.paused = false;
                    session.send(runtime::Command::Pause(false));
                }
            }
        }
        if open_four {
            let missing = 4usize.saturating_sub(
                self.sessions
                    .iter()
                    .filter(|session| !session.closed)
                    .count(),
            );
            if let Some(mut index) = self.sessions.iter().rposition(|session| !session.closed) {
                for _ in 0..missing {
                    self.spawn_from(index);
                    index = self.sessions.len() - 1;
                }
            }
            tile = true;
        }
        if tile {
            let count = self
                .sessions
                .iter()
                .filter(|session| !session.closed)
                .count();
            let columns = (count as f32).sqrt().ceil().max(1.0) as usize;
            let rows = count.div_ceil(columns);
            let monitor = ctx
                .input(|i| i.viewport().monitor_size)
                .unwrap_or(egui::vec2(1440.0, 900.0));
            self.tile_columns = columns;
            self.window_size = Pair(
                (monitor.x / columns as f32 - 8.0).max(96.0),
                ((monitor.y - 50.0) / rows as f32 - 32.0).max(96.0),
            );
            self.window_origin = Pair(4.0, 32.0);
            for (cell, (index, session)) in self
                .sessions
                .iter()
                .enumerate()
                .filter(|(_, session)| !session.closed)
                .enumerate()
            {
                let id = if index == 0 {
                    egui::ViewportId::ROOT
                } else {
                    egui::ViewportId::from_hash_of(("fictrac", session.id))
                };
                ctx.send_viewport_cmd_to(
                    id,
                    egui::ViewportCommand::InnerSize(egui::vec2(
                        self.window_size.0,
                        self.window_size.1,
                    )),
                );
                ctx.send_viewport_cmd_to(
                    id,
                    egui::ViewportCommand::OuterPosition(egui::pos2(
                        self.window_origin.0 + (cell % columns) as f32 * (self.window_size.0 + 8.0),
                        self.window_origin.1
                            + (cell / columns) as f32 * (self.window_size.1 + 32.0),
                    )),
                );
            }
        }
    }
}
impl eframe::App for Desktop {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.refresh_camera_occupancy();
        if let Some(mut bridge) = self.control.take() {
            bridge.poll(self);
            self.control = Some(bridge);
        }
        if !self.sessions[0].closed {
            self.sessions[0].show(ctx);
        }
        let mut cell = usize::from(!self.sessions[0].closed);
        for session in self.sessions.iter_mut().skip(1) {
            if session.closed {
                continue;
            }
            let id = egui::ViewportId::from_hash_of(("fictrac", session.id));
            ctx.show_viewport_immediate(
                id,
                egui::ViewportBuilder::default()
                    .with_title(session.window_title())
                    .with_inner_size([self.window_size.0, self.window_size.1])
                    .with_position([
                        self.window_origin.0
                            + (cell % self.tile_columns) as f32 * (self.window_size.0 + 8.0),
                        self.window_origin.1
                            + (cell / self.tile_columns) as f32 * (self.window_size.1 + 32.0),
                    ])
                    .with_max_inner_size(session.max_window_inner_size)
                    .with_min_inner_size([96.0, 96.0]),
                |ctx, class| {
                    session.native_window = class != egui::ViewportClass::Embedded;
                    session.show(ctx);
                },
            );
            cell += 1;
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
                4usize.saturating_sub(self.sessions.iter().filter(|s| !s.closed).count())
            } else {
                requested
            };
            for _ in 0..n {
                self.spawn_from(i);
                i = self.sessions.len() - 1;
            }
        }
        self.rig_manager(ctx);
        if self.sessions.iter().all(|s| s.closed) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if self.sessions[0].closed {
            // eframe owns a root event-loop window. Closing a tracking session
            // must not close other sessions: retain only a hidden event host,
            // and release this session's capture, outputs and image memory.
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if !self.root_hidden {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                let root = &mut self.sessions[0];
                root.worker = None;
                root.show_settings = false;
                root.snapshot = runtime::Snapshot::default();
                root.textures.clear();
                root.texture_image = None;
                self.root_hidden = true;
            }
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
            let instances:Vec<_>=self.sessions.iter().filter(|s| !s.closed).map(|s|serde_json::json!({"id":s.id,"frames":s.snapshot.frames,"source":s.source_edit,"config_path":s.path,"paused":s.snapshot.paused,"native_window":s.native_window,"window_rect":s.window_rect,"window_title":s.window_title(),"revision":s.snapshot.revision,"notice":s.snapshot.notice,"error":s.error,"config_text":s.history.text})).collect();
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
fn app_icon() -> Result<egui::IconData, String> {
    let icon = image::load_from_memory(include_bytes!("../../../assets/hejjetrak.png"))
        .map_err(|e| format!("Bundled application icon: {e}"))?
        .into_rgba8();
    Ok(egui::IconData {
        width: icon.width(),
        height: icon.height(),
        rgba: icon.into_raw(),
    })
}
#[cfg(target_os = "linux")]
fn linux_graphics_backends(explicit: Option<wgpu::Backends>, x11: bool) -> wgpu::Backends {
    explicit.unwrap_or(if x11 {
        wgpu::Backends::GL
    } else {
        wgpu::Backends::PRIMARY | wgpu::Backends::GL
    })
}

#[cfg(target_os = "linux")]
fn linux_use_x11(display: bool, requested: Option<&str>) -> Result<bool, String> {
    match requested {
        None => Ok(display),
        Some("x11") if display => Ok(true),
        Some("x11") => Err("WINIT_UNIX_BACKEND=x11 requires a nonempty DISPLAY; enable X11/XWayland or unset the override".into()),
        Some("wayland") => Ok(false),
        Some(other) => Err(format!("WINIT_UNIX_BACKEND={other:?} is unsupported; choose x11 or wayland, or unset it")),
    }
}

#[cfg(target_os = "linux")]
fn x11_bootstrap_needed(
    x11: bool,
    wayland_display: Option<&str>,
    wayland_socket: Option<&str>,
) -> bool {
    x11 && (wayland_display != Some("") || wayland_socket.is_some())
}

#[cfg(target_os = "linux")]
fn bootstrap_linux_display() -> Result<(), String> {
    use std::os::unix::process::CommandExt;
    let requested = std::env::var("WINIT_UNIX_BACKEND").ok();
    let display = std::env::var("DISPLAY").is_ok_and(|value| !value.is_empty());
    let x11 = linux_use_x11(display, requested.as_deref())?;
    let wayland_display = std::env::var("WAYLAND_DISPLAY").ok();
    let wayland_socket = std::env::var("WAYLAND_SOCKET").ok();
    if x11_bootstrap_needed(x11, wayland_display.as_deref(), wayland_socket.as_deref()) {
        // wgpu's GLES instance probes Wayland independently of winit. Align
        // both before creating any windows or capture threads. Replacing this
        // same executable safely applies a private environment without unsafe
        // process-wide environment mutation or any external media runtime.
        // An empty display is intentional: an absent WAYLAND_DISPLAY can
        // connect to the default socket anyway. Remove WAYLAND_SOCKET rather
        // than giving that numeric inherited-FD variable an empty value.
        let executable = std::env::current_exe()
            .map_err(|error| format!("Cannot locate the application for X11 startup: {error}"))?;
        let error = std::process::Command::new(executable)
            .args(std::env::args_os().skip(1))
            .env("WAYLAND_DISPLAY", "")
            .env_remove("WAYLAND_SOCKET")
            .stdin(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .exec();
        return Err(format!(
            "Cannot start the compatible X11 graphics session: {error}"
        ));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn configure_graphics(mut options: eframe::NativeOptions) -> Result<eframe::NativeOptions, String> {
    let requested = std::env::var("WINIT_UNIX_BACKEND").ok();
    let display = std::env::var("DISPLAY").is_ok_and(|value| !value.is_empty());
    let x11 = linux_use_x11(display, requested.as_deref())?;
    if x11 {
        // The tested hybrid Vulkan driver loses surface capabilities on
        // resize, and Wayland EGL can block indefinitely in presentation.
        // X11/XWayland + GLES avoids both observed driver paths. Preserve
        // explicit backend overrides and native Wayland-only installations.
        eprintln!("Linux window system: X11/XWayland");
        options.event_loop_builder = Some(Box::new(|builder| {
            use winit::platform::x11::EventLoopBuilderExtX11;
            builder.with_x11();
        }));
    } else if requested.as_deref() == Some("wayland") {
        options.event_loop_builder = Some(Box::new(|builder| {
            use winit::platform::wayland::EventLoopBuilderExtWayland;
            builder.with_wayland();
        }));
    }
    if let eframe::egui_wgpu::WgpuSetup::CreateNew(setup) = &mut options.wgpu_options.wgpu_setup {
        setup.instance_descriptor.backends =
            linux_graphics_backends(wgpu::Backends::from_env(), x11);
    }
    Ok(options)
}

#[cfg(not(target_os = "linux"))]
fn configure_graphics(options: eframe::NativeOptions) -> Result<eframe::NativeOptions, String> {
    Ok(options)
}

fn run() -> Result<(), String> {
    let args = Args::parse();
    if !(1..=16).contains(&args.windows) {
        return Err("--windows must be 1..16".into());
    }
    if args
        .grid
        .is_some_and(|g| args.windows > (g.0 * g.1) as usize)
    {
        return Err("Grid has fewer cells than requested windows".into());
    }
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
    #[cfg(target_os = "linux")]
    bootstrap_linux_display()?;
    let options = configure_graphics(eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("HejjéTrac · VR1")
            .with_icon(app_icon()?)
            .with_inner_size([520.0, 480.0])
            .with_max_inner_size([4096.0, 4096.0])
            .with_min_inner_size([96.0, 96.0]),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    })?;
    eframe::run_native(
        "HejjéTrac",
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
            eprintln!("hejjetrac: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod arguments_tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_preview_prefers_x11_gles_and_preserves_explicit_backends() -> Result<(), String> {
        assert!(linux_use_x11(true, None)?);
        assert!(!linux_use_x11(false, None)?);
        assert!(!linux_use_x11(true, Some("wayland"))?);
        assert!(linux_use_x11(false, Some("x11")).is_err());
        assert!(linux_use_x11(true, Some("unknown")).is_err());
        assert!(x11_bootstrap_needed(true, Some("wayland-0"), None));
        assert!(x11_bootstrap_needed(true, None, None));
        assert!(!x11_bootstrap_needed(true, Some(""), None));
        assert!(x11_bootstrap_needed(true, Some(""), Some("")));
        assert!(!x11_bootstrap_needed(false, Some("wayland-0"), None));
        assert_eq!(linux_graphics_backends(None, true), wgpu::Backends::GL);
        assert_eq!(
            linux_graphics_backends(Some(wgpu::Backends::VULKAN), true),
            wgpu::Backends::VULKAN
        );
        assert_eq!(
            linux_graphics_backends(None, false),
            wgpu::Backends::PRIMARY | wgpu::Backends::GL
        );
        Ok(())
    }

    #[test]
    fn tiny_preview_size_is_available_through_the_public_cli()
    -> Result<(), Box<dyn std::error::Error>> {
        let args = Args::try_parse_from(["hejjetrac", "--window-size", "96x96"])?;
        let size = args.window_size.ok_or("missing preview size")?;
        assert_eq!((size.0, size.1), (96.0, 96.0));
        for value in ["95x96", "96x95", "NaNx96", "96xinf", "8193x96"] {
            assert!(Args::try_parse_from(["hejjetrac", "--window-size", value]).is_err());
        }
        Ok(())
    }
}
