use crate::{
    calibration,
    icons::{self, Icon},
    runtime::Command,
    session::{Session, Tool},
};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Stroke, TextureOptions, Vec2};
use fictrac_core::Config;
use fictrac_io::{load_config_text, parameter_description};
use glam::{DQuat, DVec3};
const YELLOW: Color32 = Color32::from_rgb(230, 235, 35);
impl Session {
    pub fn show(&mut self, ctx: &egui::Context) {
        self.update();
        let title = format!("hejjetrak · {}", self.display_name());
        if self.last_title != title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }
        self.window_rect = ctx
            .input(|i| i.viewport().outer_rect)
            .map(|r| [r.min.x, r.min.y, r.width(), r.height()]);
        if ctx.input(|i| i.viewport().close_requested()) && !self.closed {
            if self.history.text != self.saved {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.close_pending = true;
            } else {
                self.closed = true;
            }
        }
        if self.close_pending {
            egui::Modal::new(egui::Id::new(("unsaved-config", self.id))).show(ctx, |ui| {
                ui.heading("Unsaved configuration");
                ui.label("Save configuration changes before closing?");
                ui.horizontal(|ui| {
                    if ui.button("Save and close").clicked() {
                        let result = self.save(false);
                        if result.is_ok() {
                            self.closed = true;
                            self.close_pending = false;
                        }
                        self.report(result);
                    }
                    if ui.button("Discard and close").clicked() {
                        self.closed = true;
                        self.close_pending = false;
                    }
                    if ui.button("Cancel").clicked() {
                        self.close_pending = false;
                    }
                });
            });
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::S)) {
            {
                let result = self.save(false);
                self.report(result);
            };
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::N)) {
            self.spawn_request = 1;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z))
            && self.history.undo()
        {
            self.changed = Some(std::time::Instant::now());
        }
        if ctx.input_mut(|i| {
            i.consume_key(
                egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                egui::Key::Z,
            )
        }) && self.history.redo()
        {
            self.changed = Some(std::time::Instant::now());
        }
        if !ctx.wants_keyboard_input() {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::R)) {
                self.send(Command::Reset);
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F))
                && self.tool == Tool::Square
            {
                {
                    let result = self.cycle_square();
                    self.report(result);
                };
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
                self.tool = Tool::Inspect;
                self.points.clear();
                self.square_slots = None;
                self.show_settings = false;
            }
        }
        let dropped = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect::<Vec<_>>()
        });
        if let Some(path) = dropped.first() {
            if path.extension().is_some_and(|e| e == "txt") {
                {
                    let result = self.load(path.clone());
                    self.report(result);
                };
            } else {
                {
                    let result = self.video(path.clone());
                    self.report(result);
                };
            }
        }
        egui::TopBottomPanel::top(egui::Id::new(("toolbar", self.id)))
            .frame(
                egui::Frame::new()
                    .fill(ctx.style().visuals.panel_fill)
                    .inner_margin(5.0),
            )
            .show(ctx, |ui| {
                self.toolbar(ui);
            });
        egui::TopBottomPanel::bottom(egui::Id::new(("status", self.id))).show(ctx, |ui| {
            self.status_bar(ui);
        });
        self.refresh_textures(ctx);
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(Color32::from_rgb(12, 15, 19)))
            .show(ctx, |ui| {
                let rect = ui.available_rect_before_wrap();
                if self.show_diagnostics {
                    let width = rect.width().min(rect.height() * 4.0 / 3.0);
                    let size = Vec2::new(width, width * 0.75);
                    self.canvas(ui, Rect::from_center_size(rect.center(), size));
                } else {
                    self.preview(ui, rect);
                }
            });
        if self.show_settings {
            let mut close = false;
            let position = ctx
                .input(|i| i.viewport().outer_rect)
                .map(|r| egui::pos2(r.right() + 8.0, r.top()));
            let mut builder = egui::ViewportBuilder::default()
                .with_title(format!("hejjetrak · {} configuration", self.display_name()))
                .with_inner_size([440.0, 640.0])
                .with_min_inner_size([340.0, 360.0]);
            if let Some(position) = position {
                builder = builder.with_position(position);
            }
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of(("settings", self.id)),
                builder,
                |ctx, _class| {
                    if ctx.input(|i| i.viewport().close_requested()) {
                        close = true;
                    }
                    egui::CentralPanel::default().show(ctx, |ui| {
                        egui::ScrollArea::vertical().show(ui, |ui| self.settings(ui));
                    });
                    if !ctx.wants_keyboard_input() {
                        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F))
                            && self.tool == Tool::Square
                        {
                            let result = self.cycle_square();
                            self.report(result);
                        }
                        if ctx
                            .input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
                        {
                            close = true;
                        }
                    }
                },
            );
            if close {
                self.show_settings = false;
            }
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(16));
    }
    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| self.toolbar_row(ui));
    }
    fn toolbar_row(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().item_spacing.x = 3.0;
        if ui.available_width() > 330.0 {
            ui.strong(self.display_name()).on_hover_text(
                self.path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "Independent configuration".into()),
            );
            ui.separator();
        }
        if icons::button(
            ui,
            Icon::Reset,
            false,
            "Reset tracking (Shift+R). Retains source time and measured integrated displacement.",
        )
        .clicked()
        {
            self.send(Command::Reset);
        }
        if icons::button(
            ui,
            Icon::Record,
            self.snapshot.recording,
            if self.snapshot.recording {
                "Stop and finalize recording"
            } else {
                "Start recording immediately"
            },
        )
        .clicked()
        {
            if self.snapshot.recording {
                self.send(Command::StopRecord);
            } else {
                self.begin_recording();
            }
        }
        let record_menu = ui
            .menu_button(" ", |ui| {
                self.recording_menu(ui);
            })
            .response;
        let c = record_menu.rect.center();
        ui.painter().add(egui::Shape::line(
            vec![
                c + Vec2::new(-3.0, -1.5),
                c + Vec2::new(0.0, 1.5),
                c + Vec2::new(3.0, -1.5),
            ],
            Stroke::new(1.4_f32, ui.visuals().text_color()),
        ));
        record_menu.on_hover_text("Choose raw video, processed video, both, or data only");
        ui.menu_button("Source", |ui| {
            self.source_controls(ui);
        })
        .response
        .on_hover_text("Select a camera or video; playback controls");
        if icons::button(
            ui,
            Icon::Diagnostics,
            self.show_diagnostics,
            "Toggle the six FicTrac diagnostic panels",
        )
        .clicked()
        {
            self.show_diagnostics = !self.show_diagnostics;
        }
        if icons::button(
            ui,
            Icon::Settings,
            self.show_settings,
            "Edit configuration and calibrate",
        )
        .clicked()
        {
            self.show_settings = !self.show_settings;
        }
        ui.menu_button("…", |ui| {
            self.window_menu(ui);
        })
        .response
        .on_hover_text("Window, view and configuration actions");
    }
    fn status_bar(&mut self, ui: &mut egui::Ui) {
        let quality = self.snapshot.row.as_ref().is_some_and(|r| r.accepted);
        let overloaded = self.snapshot.headroom_percent < 0.0;
        let color = if self.snapshot.ended {
            Color32::GRAY
        } else if !quality {
            Color32::from_rgb(238, 180, 80)
        } else if overloaded {
            Color32::LIGHT_RED
        } else {
            Color32::from_rgb(96, 210, 158)
        };
        let text = format!(
            "{:.1} fps · {:.2} ms{}",
            self.snapshot.actual_fps,
            self.snapshot.processing_ms,
            if self.snapshot.dropped_running > 0 {
                format!(" · drop {}", self.snapshot.dropped_running)
            } else {
                String::new()
            }
        );
        let detail = format!(
            "{} frames\nFit: {:.2} ms\nDecode + tracking + diagnostics + recording + dispatch: {:.2} ms (excludes camera wait and playback pacing)\nSource: {:.2} fps · headroom {:.0}%\nCapture frames skipped: {} (includes intentional pauses)\n{}\nUDP {} · ZMQ {}\n{}\n{}",
            self.snapshot.frames,
            1000.0 / self.snapshot.processing_fps.max(1e-9),
            self.snapshot.processing_ms,
            self.snapshot.source_fps,
            self.snapshot.headroom_percent,
            self.snapshot.dropped,
            self.snapshot.notice,
            self.document()
                .map(|d| d.number("sock_port", -1.0).to_string())
                .unwrap_or_default(),
            self.document()
                .map(|d| d.number("zmq_port", -1.0).to_string())
                .unwrap_or_default(),
            self.error,
            self.fallback_notice
        );
        ui.horizontal(|ui| {
            let (indicator, _) = ui.allocate_exact_size(Vec2::new(7.0, 12.0), egui::Sense::hover());
            ui.painter().circle_filled(indicator.center(), 2.5, color);
            ui.label(egui::RichText::new(text).small().color(color))
                .on_hover_text(detail);
            if self.snapshot.paused {
                ui.small("Paused");
            } else if self.snapshot.ended {
                ui.small("Done");
            } else if !quality && self.snapshot.row.is_some() {
                ui.small("Lost").on_hover_text(
                    "Frame rejected; motion is unmeasured. Reset or inspect calibration.",
                );
            }
            if ui.available_width() > 90.0
                && let Some(output) = &self.snapshot.outputs
                && let Some(zmq) = &output.zmq
            {
                let port = zmq.endpoint.rsplit(':').next().unwrap_or("");
                ui.small(format!("Z:{port}")).on_hover_text(format!(
                    "ZMQ PUB {} · UDP {}\nPublished {} · queued {} · dropped {} · failures {}\n{}",
                    zmq.endpoint,
                    output.udp_destination.as_deref().unwrap_or("off"),
                    zmq.published,
                    zmq.queue_len,
                    zmq.dropped,
                    zmq.failures,
                    zmq.last_error.as_deref().unwrap_or("")
                ));
            }
            if !self.error.is_empty() {
                ui.colored_label(Color32::LIGHT_RED, "!")
                    .on_hover_text(&self.error);
            }
            if !self.fallback_notice.is_empty() {
                ui.label("i").on_hover_text(&self.fallback_notice);
            }
            if self.snapshot.recording {
                ui.colored_label(Color32::LIGHT_RED, "REC").on_hover_text(
                    self.snapshot
                        .recording_directory
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_default(),
                );
            }
            if self.history.text != self.saved {
                ui.small("•").on_hover_text("Unsaved configuration changes");
            }
        });
    }
    fn recording_menu(&mut self, ui: &mut egui::Ui) {
        ui.strong("Next recording");
        ui.checkbox(&mut self.record_raw,"Raw video").on_hover_text("Original pixels at source resolution; recording starts on the next measured frame.");
        ui.checkbox(&mut self.record_debug, "Processed video")
            .on_hover_text("FicTrac diagnostic panels with measured motion and quality.");
        ui.small("Numeric data and quality are always retained.");
    }
    fn window_menu(&mut self, ui: &mut egui::Ui) {
        if ui.button("New window  ⌘/Ctrl+N").clicked() {
            self.spawn_request = 1;
            ui.close();
        }
        if ui.button("Open four windows").clicked() {
            self.spawn_request = 3;
            ui.close();
        }
        ui.checkbox(&mut self.full_image, "Full source image")
            .on_hover_text(
                "Otherwise crop around the ball. Pixels always retain their aspect ratio.",
            );
        ui.checkbox(&mut self.show_overlays, "Calibration outlines and axes")
            .on_hover_text(
                "Show ball rim, exclusions and lab axes. Trajectory remains visible in preview.",
            );
        if ui.button("Open configuration…").clicked()
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("FicTrac config", &["txt"])
                .pick_file()
        {
            {
                let result = self.load(path);
                self.report(result);
            };
            ui.close();
        }
        if ui.button("Save configuration").clicked() {
            {
                let result = self.save(false);
                self.report(result);
            };
            ui.close();
        }
        if ui.button("Save configuration as…").clicked() {
            {
                let result = self.save(true);
                self.report(result);
            };
            ui.close();
        }
    }
    fn calibration_settings(&mut self, ui: &mut egui::Ui) {
        let before = self.tool;
        ui.horizontal_wrapped(|ui| {
            self.calibration_tools(ui);
        });
        if before != self.tool {
            self.points.clear();
            self.square_slots = None;
            self.full_image = self.tool != Tool::Inspect;
            self.show_overlays = self.tool != Tool::Inspect;
            if self.tool != Tool::Inspect {
                self.paused = true;
                self.send(Command::Pause(true));
            }
        }
        ui.checkbox(
            &mut self.show_overlays,
            "Show calibration outlines and axes",
        );
        if self.tool == Tool::Square {
            ui.horizontal(|ui| {
                ui.label("Lab plane");
                for plane in ["xy", "yz", "xz"] {
                    ui.selectable_value(&mut self.plane, plane.into(), plane.to_uppercase());
                }
            });
            if ui.button("Flip fitted orientation  F").clicked() {
                {
                    let result = self.cycle_square();
                    self.report(result);
                };
            }
        }
        match self.tool {
            Tool::Rim => {
                ui.label("Click 3+ rim points, then Fit. Right-click deletes nearest point.");
            }
            Tool::Exclude => {
                ui.label("Click polygon corners, then Enter. Right-click deletes nearest point.");
            }
            Tool::Square => {
                ui.label("Click TL → TR → BR → BL. The fitted square remains visible. Press F to refit the alternate orientation.");
            }
            Tool::Edit => {
                ui.label("Drag a saved point; right-click deletes it. Invalid geometry leaves the config intact.");
            }
            _ => {
                ui.label("Choose a tool to edit a paused source frame.");
            }
        }
        ui.horizontal(|ui| {
            if ui.button("Fit / finish").clicked() {
                {
                    let result = self.finish_points();
                    self.report(result);
                };
            }
            if ui.button("Cancel points").clicked() {
                self.points.clear();
                self.square_slots = None;
            }
            if ui.button("Done / resume").clicked() {
                self.tool = Tool::Inspect;
                self.show_overlays = false;
                self.full_image = false;
                self.paused = false;
                self.send(Command::Pause(false));
            }
        });
    }
    fn config_text(&mut self, ui: &mut egui::Ui) {
        let mut text = self.history.text.clone();
        if ui
            .add(
                egui::TextEdit::multiline(&mut text)
                    .code_editor()
                    .desired_rows(18)
                    .desired_width(f32::INFINITY),
            )
            .changed()
        {
            self.edit(text);
        }
        ui.checkbox(&mut self.auto_apply,"Apply edits automatically").on_hover_text("Validated edits start a numerical epoch after 400 ms; source position and time are retained.");
    }
    fn calibration_tools(&mut self, ui: &mut egui::Ui) {
        for (tool, label, help) in [
            (Tool::Inspect, "View", "No geometry edits"),
            (
                Tool::Rim,
                "Ball rim",
                "Left-click rim points; right-click removes the nearest point",
            ),
            (
                Tool::Exclude,
                "Exclude",
                "Left-click polygon vertices; Enter finishes; right-click removes a vertex",
            ),
            (
                Tool::Square,
                "Lab square",
                "Click TL, TR, BR, BL; F tries the alternate fitted orientation",
            ),
            (
                Tool::Edit,
                "Move points",
                "Drag saved rim, exclusion or square points",
            ),
        ] {
            ui.selectable_value(&mut self.tool, tool, label)
                .on_hover_text(help);
        }
    }
    fn source_controls(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.strong("Cameras");
            if icons::button(ui, Icon::Refresh, false, "Refresh available cameras").clicked() {
                self.refresh_cameras();
            }
        });
        let cameras = self.cameras.clone();
        egui::ScrollArea::vertical()
            .max_height(180.0)
            .id_salt(("camera-list", self.id))
            .show(ui, |ui| {
                if cameras.is_empty() {
                    ui.label("No cameras detected");
                }
                for camera in cameras {
                    if ui
                        .selectable_label(
                            self.source_edit == camera.id.to_string(),
                            format!("{} · {}", camera.id, camera.name),
                        )
                        .clicked()
                    {
                        {
                            let result = self.fields(&[("src_fn", camera.id.to_string())]);
                            self.report(result);
                        };
                        {
                            let result = self.start();
                            self.report(result);
                        };
                        ui.close();
                    }
                }
            });
        if !self.camera_error.is_empty() {
            ui.colored_label(Color32::LIGHT_RED, &self.camera_error);
        }
        ui.separator();
        if ui
            .button("Open video…")
            .on_hover_text("Or drop a video directly onto this window")
            .clicked()
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("Video", &["mp4", "mov", "avi", "y4m"])
                .pick_file()
        {
            {
                let result = self.video(path);
                self.report(result);
            };
            ui.close();
        }
        ui.label(egui::RichText::new(&self.source_edit).small());
        ui.horizontal(|ui| {
            if ui
                .button(if self.paused { "Play" } else { "Pause" })
                .clicked()
            {
                self.paused = !self.paused;
                self.send(Command::Pause(self.paused));
            }
            if ui.button("Step").clicked() {
                self.paused = true;
                self.send(Command::Step);
            }
            if ui.button("Replay / reconnect").clicked() {
                {
                    let result = self.start();
                    self.report(result);
                };
            }
        });
        ui.small("Video plays at source speed.");
    }
    fn settings(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui
                .button("Save")
                .on_hover_text("Save exact config text (⌘/Ctrl+S)")
                .clicked()
            {
                {
                    let result = self.save(false);
                    self.report(result);
                };
            }
            if ui.button("Undo").clicked() && self.history.undo() {
                self.changed = Some(std::time::Instant::now());
            }
            if ui.button("Redo").clicked() && self.history.redo() {
                self.changed = Some(std::time::Instant::now());
            }
            if ui
                .button("Apply")
                .on_hover_text(
                    "Apply numerical edits; starts a new tracking epoch without rewinding video",
                )
                .clicked()
            {
                {
                    let result = self.apply();
                    self.report(result);
                };
            }
        });
        if self.disk_conflict {
            ui.colored_label(
                Color32::YELLOW,
                "File changed on disk; save conflict preserved.",
            );
            if ui.button("Reload disk version").clicked()
                && let Some(path) = self.path.clone()
            {
                {
                    let result = self.load(path);
                    self.report(result);
                };
            }
        }
        egui::CollapsingHeader::new("Source & camera").show(ui, |ui| {
            self.source_controls(ui);
            self.camera_controls(ui);
        });
        egui::CollapsingHeader::new("Graphical calibration")
            .default_open(false)
            .show(ui, |ui| {
                self.calibration_settings(ui);
            });
        let fields = self.document().map(|d| {
            d.fields()
                .map(|(k, v)| (k.to_owned(), v.to_owned()))
                .collect::<Vec<_>>()
        });
        match fields {
            Ok(fields) => {
                for category in [
                    "Geometry & calibration",
                    "Tracking & quality",
                    "Output & recording",
                    "Advanced & compatibility",
                ] {
                    egui::CollapsingHeader::new(category).show(ui, |ui| {
                        let mut changes = vec![];
                        egui::Grid::new(("fields", self.id, category))
                            .num_columns(2)
                            .spacing([12.0, 6.0])
                            .show(ui, |ui| {
                                for (key, original) in fields
                                    .iter()
                                    .filter(|(key, _)| field_category(key) == category)
                                {
                                    let mut value = original.clone();
                                    let (status, help) = parameter_description(key);
                                    let tooltip = format!("{status}: {help}");
                                    ui.label(key).on_hover_text(&tooltip);
                                    let response = if value == "y" || value == "n" {
                                        let mut checked = value == "y";
                                        let r = ui.checkbox(&mut checked, "");
                                        value = if checked { "y" } else { "n" }.into();
                                        r
                                    } else {
                                        ui.add(
                                            egui::TextEdit::singleline(&mut value)
                                                .desired_width(170.0)
                                                .id_salt((self.id, key)),
                                        )
                                    };
                                    let changed = response.changed();
                                    response.on_hover_text(&tooltip);
                                    if changed {
                                        changes.push((key.as_str(), value));
                                    }
                                    ui.end_row();
                                }
                            });
                        if !changes.is_empty() {
                            {
                                let result = self.fields(&changes);
                                self.report(result);
                            };
                        }
                    });
                }
            }
            Err(error) => {
                ui.colored_label(Color32::LIGHT_RED, error);
            }
        }
        egui::CollapsingHeader::new("Exact config text").show(ui, |ui| {
            self.config_text(ui);
        });
        egui::CollapsingHeader::new("Recording & sphere templates").show(ui, |ui| {
            ui.label("Recording folder");
            ui.text_edit_singleline(&mut self.recording_path)
                .on_hover_text("Each instant recording creates a unique take directory here.");
            if ui.button("Save accumulated sphere…").clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter("PNG", &["png"])
                    .save_file()
            {
                self.send(Command::SaveMap(path));
            }
            if ui.button("Load sphere template…").clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter("PNG", &["png"])
                    .pick_file()
            {
                self.send(Command::LoadMap(path));
            }
        });
        if !self.error.is_empty() {
            ui.colored_label(Color32::LIGHT_RED, &self.error);
        }
    }
    fn cycle_square(&mut self) -> Result<(), String> {
        let image = self
            .snapshot
            .image
            .as_ref()
            .ok_or("No source image for square calibration")?;
        let (doc, pose) =
            fictrac_control::cycle_square_document(&self.document()?, image.width, image.height)?;
        self.edit(doc.text().to_owned());
        self.apply()?;
        self.status = format!(
            "Alternate square fit · ray error {:.3e} · orientation change {:.1}°",
            pose.ray_error,
            pose.orientation_change_rad.to_degrees()
        );
        Ok(())
    }
    fn camera_controls(&mut self, ui: &mut egui::Ui) {
        let Some(capabilities) = self.snapshot.camera.clone() else {
            ui.small("Select a camera to see its supported native controls.");
            return;
        };
        ui.separator();
        ui.horizontal(|ui| {
            ui.strong(&capabilities.backend);
            if icons::button(
                ui,
                Icon::Refresh,
                false,
                "Refresh native camera settings and readback",
            )
            .clicked()
            {
                self.send(Command::CameraRefresh);
            }
        });
        let current = &capabilities.current_format;
        let mut format = self.camera_edit.clone().unwrap_or_else(|| current.clone());
        egui::ComboBox::from_id_salt(("resolution",self.id)).selected_text(format!("{} × {}",current.width,current.height)).show_ui(ui,|ui|{
            let mut seen=std::collections::BTreeSet::new();
            for option in &capabilities.formats {
                if !seen.insert((option.width,option.height)){
continue;
}
                if ui.selectable_label(format.width==option.width&&format.height==option.height,format!("{} × {}",option.width,option.height)).clicked(){
                    format.width=option.width;
format.height=option.height;
                    let fps=current.fps.unwrap_or(30.0);
                    format.fps=option.frame_rates.first().map(|r|fps.clamp(r.min_fps,r.max_fps));
                }
            }
        }
).response.on_hover_text("Native output resolution. Changing format reopens the device and starts a new tracking epoch; recalibrate for the new geometry.");
        ui.horizontal(|ui| {
            ui.label("Frame rate");
            let mut fps = format.fps.unwrap_or(30.0);
            if ui
                .add(
                    egui::DragValue::new(&mut fps)
                        .range(0.1..=1000.0)
                        .speed(0.1)
                        .suffix(" fps"),
                )
                .on_hover_text(
                    "Requested native frame rate; only device-advertised rates are accepted.",
                )
                .changed()
            {
                format.fps = Some(fps);
            }
        });
        if format != *current {
            self.camera_edit = Some(format.clone());
            ui.horizontal(|ui| {
                if ui
                    .button("Apply camera format")
                    .on_hover_text(
                        "Reopen with this resolution and frame rate; starts a new tracking epoch.",
                    )
                    .clicked()
                {
                    let result = self.camera_format(format);
                    self.report(result);
                }
                if ui.button("Cancel").clicked() {
                    self.camera_edit = None;
                }
            });
        } else {
            self.camera_edit = None;
        }

        if capabilities.supported_exposure_modes.is_empty() {
            ui.add_enabled(false, egui::Label::new("Exposure mode: unavailable"))
                .on_hover_text(
                    "This device/backend does not advertise a writable native exposure mode.",
                );
        } else {
            let mut mode = capabilities.exposure_mode;
            egui::ComboBox::from_id_salt(("exposure-mode", self.id))
                .selected_text(format!("Exposure: {:?}", mode))
                .show_ui(ui, |ui| {
                    for option in &capabilities.supported_exposure_modes {
                        ui.selectable_value(&mut mode, Some(*option), format!("{option:?}"));
                    }
                });
            if mode != capabilities.exposure_mode
                && let Some(mode) = mode
            {
                self.send(Command::CameraControl(
                    fictrac_camera::CameraControlRequest::ExposureMode { mode },
                ));
            }
        }
        for (label, control) in [
            ("Exposure", capabilities.exposure),
            ("Gain", capabilities.gain),
        ] {
            if !control.writable && control.value.is_none() {
                ui.add_enabled(false, egui::Label::new(format!("{label}: unavailable")))
                    .on_hover_text(
                        control
                            .unsupported_reason
                            .unwrap_or_else(|| "Not advertised by this device".into()),
                    );
                continue;
            }
            let mut value = control.value.unwrap_or(0.0);
            ui.horizontal(|ui|{
                ui.label(label);
                let mut edit=egui::DragValue::new(&mut value).speed(control.step.unwrap_or(0.1)).suffix(format!(" {}",control.unit));
                if let (Some(min),Some(max))=(control.minimum,control.maximum){
edit=edit.range(min..=max);
}
                let response=ui.add_enabled(control.writable,edit).on_hover_text(control.unsupported_reason.clone().unwrap_or_else(||format!("Native units; range {:?}..{:?}, step {:?}. Values are read back from the device.",control.minimum,control.maximum,control.step)));
                if response.changed(){
self.send(Command::CameraControl(if label=="Exposure"{
fictrac_camera::CameraControlRequest::Exposure{
value}
}
else{
fictrac_camera::CameraControlRequest::Gain{
value}
}
));
}
            }
);
        }
        for warning in capabilities.warnings {
            ui.small(warning);
        }
    }
    fn image_uv(&self) -> Rect {
        let full = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
        if self.full_image {
            return full;
        }
        let Some(image) = &self.snapshot.image else {
            return full;
        };
        let Ok(config) = self.core_config() else {
            return full;
        };
        let Ok(geometry) = fictrac_core::Geometry::new(config.clone()) else {
            return full;
        };
        let centre = DVec3::from_array(config.sphere.centre);
        let perpendicular = if centre.y.abs() < 0.9 {
            DVec3::Y
        } else {
            DVec3::X
        };
        let u = centre.cross(perpendicular).normalize();
        let v = centre.cross(u);
        let radius = config.sphere.angular_radius;
        let points: Vec<_> = (0..=96)
            .filter_map(|i| {
                let a = i as f64 * std::f64::consts::TAU / 96.0;
                geometry
                    .project_ray(centre * radius.cos() + (u * a.cos() + v * a.sin()) * radius.sin())
            })
            .collect();
        if points.is_empty() {
            return full;
        }
        let bounds = points.iter().fold(
            [
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            ],
            |b, p| {
                [
                    b[0].min(p[0]),
                    b[1].min(p[1]),
                    b[2].max(p[0]),
                    b[3].max(p[1]),
                ]
            },
        );
        let margin = ((bounds[2] - bounds[0]).max(bounds[3] - bounds[1]) * 0.08).max(2.0);
        let min = Pos2::new(
            ((bounds[0] - margin) / image.width as f64).clamp(0.0, 1.0) as f32,
            ((bounds[1] - margin) / image.height as f64).clamp(0.0, 1.0) as f32,
        );
        let max = Pos2::new(
            ((bounds[2] + margin) / image.width as f64).clamp(0.0, 1.0) as f32,
            ((bounds[3] + margin) / image.height as f64).clamp(0.0, 1.0) as f32,
        );
        if max.x <= min.x || max.y <= min.y {
            full
        } else {
            Rect::from_min_max(min, max)
        }
    }
    fn preview(&mut self, ui: &mut egui::Ui, rect: Rect) {
        if self
            .document()
            .is_ok_and(|doc| doc.get("do_display") == Some("n") && !doc.boolean("save_debug"))
        {
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                "do_display = n · tracking continues",
                FontId::proportional(13.0),
                Color32::GRAY,
            );
            return;
        }
        let Some(image) = &self.snapshot.image else {
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                "Opening source…\nDrop a video to begin",
                FontId::proportional(14.0),
                Color32::GRAY,
            );
            return;
        };
        let uv = self.image_uv();
        let target = fit_image(rect, image.width, image.height, uv);
        if let Some(texture) = self.textures.first() {
            ui.painter().image(texture.id(), target, uv, Color32::WHITE);
        }
        self.interact_source(ui, target, uv);
        if !self.snapshot.path.is_empty() && rect.width() > 300.0 {
            let path = Rect::from_min_size(
                Pos2::new(rect.right() - 110.0, rect.bottom() - 90.0),
                Vec2::new(104.0, 84.0),
            );
            ui.painter()
                .rect_filled(path, 5.0, Color32::from_black_alpha(150));
            self.draw_path(ui, path.shrink(3.0));
        }
    }
    fn refresh_textures(&mut self, ctx: &egui::Context) {
        let key = self
            .snapshot
            .frames
            .wrapping_add(self.snapshot.revision.wrapping_shl(32))
            .wrapping_add(u64::from(self.show_diagnostics).wrapping_shl(63));
        if self.texture_frame == key {
            return;
        }
        let Some(frame) = &self.snapshot.image else {
            return;
        };
        if frame.rgb.len() != frame.width * frame.height * 3 {
            return;
        }
        let mut images = vec![egui::ColorImage::from_rgb(
            [frame.width, frame.height],
            &frame.rgb,
        )];
        if self.show_diagnostics
            && let Some(d) = &frame.debug
        {
            images.extend([
                egui::ColorImage::from_gray([d.roi_size, d.roi_size], &d.roi),
                egui::ColorImage::from_gray([d.roi_size, d.roi_size], &d.diff),
                egui::ColorImage::from_gray([d.map_width, d.map_height], &d.instant),
                egui::ColorImage::from_gray([d.map_width, d.map_height], &d.accumulated),
            ]);
        }
        self.textures.truncate(images.len());
        for (i, image) in images.into_iter().enumerate() {
            if let Some(texture) = self.textures.get_mut(i) {
                texture.set(image, TextureOptions::LINEAR);
            } else {
                self.textures.push(ctx.load_texture(
                    format!("session-{}-{i}", self.id),
                    image,
                    TextureOptions::LINEAR,
                ));
            }
        }
        self.texture_frame = key;
    }
    fn core_config(&self) -> Result<Config, String> {
        let (width, height) = self
            .snapshot
            .image
            .as_ref()
            .map(|i| (i.width, i.height))
            .unwrap_or((384, 288));
        let mut doc = self.document()?;
        doc.set("src_fps", "-1").map_err(|e| e.to_string())?;
        load_config_text(doc.text(), width, height)
            .map(|l| l.config)
            .map_err(|e| e.to_string())
    }
    fn canvas(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let p = ui.painter();
        p.rect_filled(rect, 0.0, Color32::BLACK);
        let cell = rect.width() / 4.0;
        let r = |x: f32, y: f32, w: f32, h: f32| {
            Rect::from_min_size(
                rect.min + Vec2::new(x * cell, y * cell),
                Vec2::new(w * cell, h * cell),
            )
        };
        let source = r(0.0, 0.0, 2.0, 2.0);
        let roi = r(2.0, 0.0, 1.0, 1.0);
        let diff = r(3.0, 0.0, 1.0, 1.0);
        let instant = r(2.0, 1.0, 2.0, 1.0);
        let path = r(0.0, 2.0, 2.0, 1.0);
        let accumulated = r(2.0, 2.0, 2.0, 1.0);
        let display = self
            .document()
            .map(|d| d.get("do_display") != Some("n") || d.boolean("save_debug"))
            .unwrap_or(true);
        if !display {
            p.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "do_display = n · tracking continues",
                FontId::proportional(18.0),
                Color32::WHITE,
            );
            return;
        }
        let uv = self.image_uv();
        for (i, target) in [
            (0, source),
            (1, roi),
            (2, diff),
            (3, instant),
            (4, accumulated),
        ] {
            if let Some(texture) = self.textures.get(i) {
                p.image(
                    texture.id(),
                    if let Some(texture) = self.textures.get(i) {
                        let [w, h] = texture.size();
                        fit_image(
                            target,
                            w,
                            h,
                            if i == 0 {
                                uv
                            } else {
                                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0))
                            },
                        )
                    } else {
                        target
                    },
                    if i == 0 {
                        uv
                    } else {
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0))
                    },
                    Color32::WHITE,
                );
            }
        }
        self.draw_path(ui, path);
        for target in [source, roi, diff, instant, path, accumulated] {
            p.rect_stroke(
                target,
                0.0,
                Stroke::new(1.0_f32, Color32::WHITE),
                egui::StrokeKind::Inside,
            );
        }
        for (target, label) in [
            (source, "a) input image"),
            (roi, "b) sphere ROI"),
            (diff, "c) warped diff"),
            (instant, "d) instant map"),
            (path, "e) flat path"),
            (accumulated, "f) accumulated map"),
        ] {
            let pos = target.left_bottom() + Vec2::new(5.0, -5.0);
            p.text(
                pos + Vec2::new(1.0, 1.0),
                Align2::LEFT_BOTTOM,
                label,
                FontId::proportional(13.0),
                Color32::BLACK,
            );
            p.text(
                pos,
                Align2::LEFT_BOTTOM,
                label,
                FontId::proportional(13.0),
                YELLOW,
            );
        }
        if self.snapshot.image.is_none() {
            p.text(
                source.center(),
                Align2::CENTER_CENTER,
                "Drop video · Open video · Connect camera",
                FontId::proportional(17.0),
                Color32::LIGHT_GRAY,
            );
        }
        let source = if let Some(image) = &self.snapshot.image {
            fit_image(source, image.width, image.height, uv)
        } else {
            source
        };
        self.interact_source(ui, source, uv);
    }
    fn draw_path(&self, ui: &egui::Ui, rect: Rect) {
        if self.snapshot.path.len() < 2 {
            return;
        }
        let mut min = [f64::INFINITY; 2];
        let mut max = [f64::NEG_INFINITY; 2];
        for p in self.snapshot.path.iter() {
            for i in 0..2 {
                min[i] = min[i].min(p[i]);
                max[i] = max[i].max(p[i]);
            }
        }
        let scale = ((rect.width() - 24.0) as f64 / (max[1] - min[1]).max(0.01))
            .min((rect.height() - 30.0) as f64 / (max[0] - min[0]).max(0.01));
        let points: Vec<_> = self
            .snapshot
            .path
            .iter()
            .map(|p| {
                Pos2::new(
                    rect.center().x + ((p[1] - (min[1] + max[1]) * 0.5) * scale) as f32,
                    rect.center().y - ((p[0] - (min[0] + max[0]) * 0.5) * scale) as f32,
                )
            })
            .collect();
        ui.painter().add(egui::Shape::line(
            points,
            Stroke::new(1.3_f32, Color32::WHITE),
        ));
    }
    fn interact_source(&mut self, ui: &mut egui::Ui, rect: Rect, uv: Rect) {
        let Some(image) = &self.snapshot.image else {
            return;
        };
        let w = image.width as f64;
        let h = image.height as f64;
        let to_screen = |p: [f64; 2]| {
            Pos2::new(
                rect.left()
                    + ((p[0] / w - uv.left() as f64) / uv.width() as f64) as f32 * rect.width(),
                rect.top()
                    + ((p[1] / h - uv.top() as f64) / uv.height() as f64) as f32 * rect.height(),
            )
        };
        let to_source = |p: Pos2| {
            [
                ((p.x - rect.left()) as f64 * f64::from(uv.width()) * w / f64::from(rect.width())
                    + f64::from(uv.left()) * w)
                    .clamp(0.0, w - 1.0),
                ((p.y - rect.top()) as f64 * f64::from(uv.height()) * h / f64::from(rect.height())
                    + f64::from(uv.top()) * h)
                    .clamp(0.0, h - 1.0),
            ]
        };
        let response = ui.interact(
            rect,
            ui.id().with("source-calibration"),
            egui::Sense::click_and_drag(),
        );
        let painter = ui.painter_at(rect);
        let mut rim = vec![];
        let mut polygons = vec![];
        let mut square = vec![];
        let mut square_plane = "xy".to_owned();
        if let Ok(doc) = self.document() {
            let key = doc.get("c2a_src").unwrap_or("c2a_cnrs_xy");
            square_plane = key.strip_prefix("c2a_cnrs_").unwrap_or("xy").into();
            if let Ok(values) = doc.vector::<Vec<f64>>(key) {
                square = values.as_chunks::<2>().0.to_vec();
            }
            if let Ok(values) = doc.vector::<Vec<f64>>("roi_circ") {
                rim = values.as_chunks::<2>().0.to_vec();
            }
            if let Ok(values) = doc.vector::<Vec<Vec<f64>>>("roi_ignr") {
                polygons = values
                    .into_iter()
                    .map(|v| v.as_chunks::<2>().0.to_vec())
                    .collect::<Vec<_>>();
            }
        }
        if let Ok(config) = self.core_config() {
            let centre = DVec3::from_array(config.sphere.centre);
            let f = h * 0.5 / (config.vfov_deg.to_radians() * 0.5).tan();
            let u = centre
                .cross(if centre.y.abs() < 0.9 {
                    DVec3::Y
                } else {
                    DVec3::X
                })
                .normalize();
            let v = centre.cross(u);
            let radius = config.sphere.angular_radius;
            let geometry = fictrac_core::Geometry::new(config.clone()).ok();
            let outline: Vec<_> = (0..=100)
                .filter_map(|i| {
                    let a = i as f64 * std::f64::consts::TAU / 100.0;
                    let ray = centre * radius.cos() + (u * a.cos() + v * a.sin()) * radius.sin();
                    geometry
                        .as_ref()
                        .and_then(|g| g.project_ray(ray))
                        .map(to_screen)
                })
                .collect();
            if self.show_overlays || self.tool != Tool::Inspect {
                painter.add(egui::Shape::line(
                    outline,
                    Stroke::new(1.3_f32, Color32::LIGHT_GREEN),
                ));
            }
            let origin = [
                f * centre.x / centre.z + w * 0.5,
                f * centre.y / centre.z + h * 0.5,
            ];
            let lab = DQuat::from_scaled_axis(DVec3::from_array(config.camera_to_lab));
            let basis = if config.legacy_roi_coordinates {
                DQuat::from_rotation_arc(centre, DVec3::Z)
            } else {
                DQuat::IDENTITY
            };
            if let (Some(row), Ok(geometry)) = (
                &self.snapshot.row,
                fictrac_core::Geometry::new(config.clone()),
            ) {
                let rotation = DQuat::from_scaled_axis(DVec3::from_array(row.absolute_camera));
                let mut last = None;
                for material in self.snapshot.contact_trace.iter() {
                    let p = basis.inverse() * (rotation * DVec3::from_array(*material));
                    let pixel = geometry.project(p).map(to_screen);
                    if let (Some(a), Some(b)) = (last, pixel) {
                        painter.line_segment(
                            [a, b],
                            Stroke::new(1.4_f32, Color32::from_rgb(20, 230, 235)),
                        );
                    }
                    last = pixel;
                }
            }
            if self.show_overlays || self.tool != Tool::Inspect {
                for (axis, color, label) in [
                    (DVec3::X, Color32::RED, "X forward"),
                    (DVec3::Y, Color32::GREEN, "Y right"),
                    (DVec3::Z, Color32::LIGHT_BLUE, "Z down"),
                ] {
                    let a = basis.inverse() * (lab.inverse() * axis);
                    let scale = f * radius.tan() * 0.8;
                    let start = to_screen(origin);
                    let end = to_screen([origin[0] + a.x * scale, origin[1] + a.y * scale]);
                    painter.arrow(start, end - start, Stroke::new(1.4_f32, color));
                    painter.text(
                        end,
                        Align2::LEFT_BOTTOM,
                        label,
                        FontId::proportional(10.0),
                        color,
                    );
                }
            }
        }
        if self.show_overlays || self.tool != Tool::Inspect {
            for polygon in &polygons {
                let mut pts: Vec<_> = polygon.iter().map(|p| to_screen(*p)).collect();
                if let Some(first) = pts.first().copied() {
                    pts.push(first);
                }
                painter.add(egui::Shape::line(
                    pts,
                    Stroke::new(1.6_f32, Color32::LIGHT_RED),
                ));
            }
            if self.tool != Tool::Inspect {
                for p in &rim {
                    painter.circle_filled(to_screen(*p), 3.5, Color32::LIGHT_GREEN);
                }
                for polygon in &polygons {
                    for p in polygon {
                        painter.circle_filled(to_screen(*p), 3.5, Color32::LIGHT_RED);
                    }
                }
            }
        }
        if (self.show_overlays || self.tool == Tool::Square || self.tool == Tool::Edit)
            && !(self.tool == Tool::Square && self.square_slots.is_some())
            && let Ok(doc) = self.document()
        {
            let key = doc.get("c2a_src").unwrap_or("c2a_cnrs_xy");
            if let Ok(values) = doc.vector::<Vec<f64>>(key) {
                let mut square: Vec<_> = values
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|p| to_screen(*p))
                    .collect();
                for (index, point) in square.iter().enumerate() {
                    painter.circle_filled(*point, 3.5, Color32::from_rgb(220, 160, 255));
                    painter.text(
                        *point + Vec2::new(5.0, -5.0),
                        Align2::LEFT_BOTTOM,
                        ["TL", "TR", "BR", "BL"].get(index).copied().unwrap_or(""),
                        FontId::proportional(11.0),
                        Color32::WHITE,
                    );
                }
                if let Some(first) = square.first().copied() {
                    square.push(first);
                }
                painter.add(egui::Shape::line(
                    square,
                    Stroke::new(1.5_f32, Color32::from_rgb(220, 160, 255)),
                ));
            }
        }
        if self.tool == Tool::Square
            && let Some(slots) = &self.square_slots
        {
            for (index, point) in slots.iter().enumerate() {
                if let Some(point) = point {
                    painter.text(
                        to_screen(*point) + Vec2::new(5.0, -5.0),
                        Align2::LEFT_BOTTOM,
                        ["TL", "TR", "BR", "BL"][index],
                        FontId::proportional(11.0),
                        YELLOW,
                    );
                }
            }
        }
        let preview: Vec<_> = self.points.iter().map(|p| to_screen(*p)).collect();
        if preview.len() > 1 {
            painter.add(egui::Shape::line(
                preview.clone(),
                Stroke::new(2.0_f32, YELLOW),
            ));
        }
        for p in preview {
            painter.circle_filled(p, 4.0, YELLOW);
        }
        if response.clicked_by(egui::PointerButton::Secondary)
            && self.tool != Tool::Inspect
            && let Some(pos) = response.interact_pointer_pos()
        {
            if self.tool == Tool::Square
                && let Some(slots) = &mut self.square_slots
            {
                if let Some(index) = slots
                    .iter()
                    .enumerate()
                    .filter_map(|(i, p)| p.map(|p| (i, to_screen(p).distance(pos))))
                    .filter(|(_, d)| *d <= 14.0)
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .map(|v| v.0)
                {
                    slots[index] = None;
                    self.points = slots.iter().flatten().copied().collect();
                }
                return;
            }
            if let Some(index) = self
                .points
                .iter()
                .enumerate()
                .filter_map(|(i, p)| {
                    let d = to_screen(*p).distance(pos);
                    (d <= 14.0).then_some((i, d))
                })
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|v| v.0)
            {
                self.points.remove(index);
                return;
            }
            let mut best = 14.0;
            let mut selected = None;
            if matches!(self.tool, Tool::Rim | Tool::Edit) {
                for (i, p) in rim.iter().enumerate() {
                    let d = to_screen(*p).distance(pos);
                    if d < best {
                        best = d;
                        selected = Some((0, i));
                    }
                }
            }
            if matches!(self.tool, Tool::Exclude | Tool::Edit) {
                for (i, poly) in polygons.iter().enumerate() {
                    for (j, p) in poly.iter().enumerate() {
                        let d = to_screen(*p).distance(pos);
                        if d < best {
                            best = d;
                            selected = Some((i + 1, j));
                        }
                    }
                }
            }
            if matches!(self.tool, Tool::Square | Tool::Edit) {
                for (i, p) in square.iter().enumerate() {
                    let distance = to_screen(*p).distance(pos);
                    if distance < best {
                        best = distance;
                        selected = Some((usize::MAX, i));
                    }
                }
            }
            if let Some((poly, index)) = selected {
                if poly == usize::MAX {
                    let mut slots = std::array::from_fn(|i| square.get(i).copied());
                    if let Some(slot) = slots.get_mut(index) {
                        *slot = None;
                    }
                    self.points = slots.iter().flatten().copied().collect();
                    self.square_slots = Some(slots);
                    self.tool = Tool::Square;
                    self.plane = square_plane.clone();
                } else if poly == 0 {
                    rim.remove(index);
                    if rim.len() >= 3 {
                        {
                            let result = self.commit_rim(&rim);
                            self.report(result);
                        };
                    } else {
                        self.points = rim;
                        self.tool = Tool::Rim;
                        self.status="Draft rim: add points, then Fit; saved calibration retained until valid".into();
                    }
                } else if let Some(p) = polygons.get_mut(poly - 1) {
                    p.remove(index);
                    if p.len() < 3 {
                        polygons.remove(poly - 1);
                    }
                    {
                        let result = self.commit_polygons(&polygons);
                        self.report(result);
                    };
                }
            }
            return;
        }
        if self.tool == Tool::Edit {
            if response.drag_started()
                && let Some(pos) = response.interact_pointer_pos()
            {
                let mut best = 12.0;
                self.pending_drag = None;
                for (i, p) in rim.iter().enumerate() {
                    let d = to_screen(*p).distance(pos);
                    if d < best {
                        best = d;
                        self.pending_drag = Some((0, i));
                    }
                }
                for (i, polygon) in polygons.iter().enumerate() {
                    for (j, p) in polygon.iter().enumerate() {
                        let d = to_screen(*p).distance(pos);
                        if d < best {
                            best = d;
                            self.pending_drag = Some((i + 1, j));
                        }
                    }
                }
            }
            if response.drag_started()
                && let Some(pos) = response.interact_pointer_pos()
            {
                let current = self
                    .pending_drag
                    .map(|(poly, index)| {
                        if poly == 0 {
                            to_screen(rim[index]).distance(pos)
                        } else {
                            to_screen(polygons[poly - 1][index]).distance(pos)
                        }
                    })
                    .unwrap_or(12.0);
                if let Some((index, distance)) = square
                    .iter()
                    .enumerate()
                    .map(|(i, p)| (i, to_screen(*p).distance(pos)))
                    .filter(|(_, d)| *d < current)
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                {
                    let _ = distance;
                    self.pending_drag = Some((usize::MAX, index));
                }
            }
            if response.dragged()
                && let Some(pos) = response.interact_pointer_pos()
                && let Some((poly, vertex)) = self.pending_drag
            {
                if poly == usize::MAX {
                    if let Some(p) = square.get_mut(vertex) {
                        *p = to_source(pos);
                    }
                    let result = self.commit_square(&square, &square_plane);
                    self.report(result);
                } else if poly == 0 {
                    if let Some(p) = rim.get_mut(vertex) {
                        *p = to_source(pos);
                    }
                    {
                        let result = self.commit_rim(&rim);
                        self.report(result);
                    };
                } else if let Some(polygon) = polygons.get_mut(poly - 1) {
                    if let Some(p) = polygon.get_mut(vertex) {
                        *p = to_source(pos);
                    }
                    {
                        let result = self.commit_polygons(&polygons);
                        self.report(result);
                    };
                }
            }
            if response.drag_stopped() {
                self.pending_drag = None;
            }
        } else if matches!(self.tool, Tool::Rim | Tool::Exclude | Tool::Square) {
            if response.clicked()
                && let Some(pos) = response.interact_pointer_pos()
            {
                if self.tool == Tool::Square {
                    let slots = self.square_slots.get_or_insert([None; 4]);
                    let Some(slot) = slots.iter_mut().find(|p| p.is_none()) else {
                        self.error =
                            "Square has four corners; right-click a corner to replace it".into();
                        return;
                    };
                    *slot = Some(to_source(pos));
                    self.points = slots.iter().flatten().copied().collect();
                } else {
                    let maximum = if self.tool == Tool::Exclude {
                        256
                    } else {
                        4096
                    };
                    if self.points.len() >= maximum {
                        self.error = format!("This tool supports at most {maximum} points");
                        return;
                    }
                    self.points.push(to_source(pos));
                }
                if self.tool == Tool::Square && self.points.len() == 4 {
                    {
                        let result = self.finish_points();
                        self.report(result);
                    };
                }
            }
            if response.double_clicked()
                || ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter))
            {
                {
                    let result = self.finish_points();
                    self.report(result);
                };
            }
        }
    }
    fn commit_rim(&mut self, points: &[[f64; 2]]) -> Result<(), String> {
        let mut config = self.core_config().unwrap_or_else(|_| Config::default());
        if let Some(image) = &self.snapshot.image {
            config.width = image.width;
            config.height = image.height;
        }
        if let Ok(doc) = self.document() {
            config.vfov_deg = doc.number("vfov", 45.0);
        }
        let sphere = calibration::fit_rim(&config, points)?;
        self.fields(&[
            (
                "roi_circ",
                vector(&points.iter().flatten().copied().collect::<Vec<_>>()),
            ),
            ("roi_c", vector(&sphere.centre)),
            ("roi_r", format!("{:.15}", sphere.angular_radius)),
        ])
    }
    fn commit_polygons(&mut self, polygons: &[Vec<[f64; 2]>]) -> Result<(), String> {
        let values: Vec<Vec<f64>> = polygons
            .iter()
            .map(|p| p.iter().flatten().copied().collect())
            .collect();
        let text = serde_json::to_string(&values)
            .map_err(|e| e.to_string())?
            .replace('[', "{ ")
            .replace(']', " }");
        self.fields(&[("roi_ignr", text)])
    }
    fn commit_square(&mut self, points: &[[f64; 2]], plane: &str) -> Result<(), String> {
        let image = self.snapshot.image.as_ref().ok_or("No image loaded")?;
        let doc = self.document()?;
        let (r, t) = calibration::lab_from_square(
            image.width,
            image.height,
            doc.number("vfov", 45.0),
            doc.boolean("fisheye"),
            plane,
            points,
        )?;
        let key = format!("c2a_cnrs_{plane}");
        self.fields(&[
            (
                key.as_str(),
                vector(&points.iter().flatten().copied().collect::<Vec<_>>()),
            ),
            ("c2a_src", key.clone()),
            ("c2a_r", vector(&r)),
            ("c2a_t", vector(&t)),
        ])
    }
    fn finish_points(&mut self) -> Result<(), String> {
        let mut points = self.points.clone();
        points.dedup();
        if points.len() > 1 && points.first() == points.last() {
            points.pop();
        }
        match self.tool {
            Tool::Rim => self.commit_rim(&points)?,
            Tool::Exclude => {
                if points.len() < 3 {
                    return Err("An exclusion needs at least three vertices".into());
                }
                let twice_area: f64 = points
                    .iter()
                    .zip(points.iter().cycle().skip(1))
                    .map(|(a, b)| a[0] * b[1] - b[0] * a[1])
                    .sum();
                if twice_area.abs() < 1.0 {
                    return Err("An exclusion must enclose a visible area".into());
                }
                let doc = self.document()?;
                let old: Vec<Vec<f64>> = doc.vector("roi_ignr").unwrap_or_default();
                let mut polygons: Vec<_> = old
                    .into_iter()
                    .map(|v| v.as_chunks::<2>().0.to_vec())
                    .collect();
                polygons.push(points);
                self.commit_polygons(&polygons)?;
            }
            Tool::Square => {
                self.commit_square(&points, &self.plane.clone())?;
            }
            _ => return Ok(()),
        }
        self.points.clear();
        self.square_slots = None;
        self.apply()?;
        Ok(())
    }
}
fn vector(values: &[f64]) -> String {
    format!(
        "{{ {} }}",
        values
            .iter()
            .map(|v| format!("{v:.12}"))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn field_category(key: &str) -> &'static str {
    if key.starts_with("roi_") || key.starts_with("c2a_") || matches!(key, "vfov" | "fisheye") {
        "Geometry & calibration"
    } else if key.starts_with("opt_")
        || key.starts_with("thr_")
        || matches!(key, "q_factor" | "max_bad_frames" | "rust_engine")
    {
        "Tracking & quality"
    } else if key.starts_with("save_")
        || key.starts_with("sock_")
        || key.starts_with("zmq_")
        || key.starts_with("com_")
        || matches!(key, "vid_codec" | "output_fn")
    {
        "Output & recording"
    } else {
        "Advanced & compatibility"
    }
}
fn fit_image(target: Rect, width: usize, height: usize, uv: Rect) -> Rect {
    let size = Vec2::new(width as f32 * uv.width(), height as f32 * uv.height());
    if size.x <= 0.0 || size.y <= 0.0 {
        return target;
    }
    let scale = (target.width() / size.x).min(target.height() / size.y);
    Rect::from_center_size(target.center(), size * scale)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::ViewFrame;
    use fictrac_io::ConfigDocument;
    use std::sync::Arc;
    fn session() -> Result<Session, String> {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let doc = ConfigDocument::parse(
            std::fs::read_to_string(root.join("examples/upstream-config.txt"))
                .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        let mut session = Session::new(1, doc, None);
        session.snapshot.image = Some(Arc::new(ViewFrame {
            width: 384,
            height: 288,
            rgb: vec![128; 384 * 288 * 3],
            debug: None,
        }));
        Ok(session)
    }
    #[test]
    fn graphical_rim_mask_edits_round_trip_and_undo() -> Result<(), String> {
        let mut session = session()?;
        let original = session.history.text.clone();
        session.commit_rim(&[[63.0, 171.0], [81.0, 145.0], [106.0, 135.0], [150.0, 160.0]])?;
        let doc = session.document()?;
        assert!(doc.get("roi_c").is_some());
        assert!(doc.get("roi_r").is_some());
        assert!(session.history.undo());
        assert_eq!(session.history.text, original);
        assert!(session.history.redo());
        session.commit_polygons(&[vec![[10.0, 20.0], [30.0, 20.0], [30.0, 40.0]]])?;
        let values: Vec<Vec<f64>> = session
            .document()?
            .vector("roi_ignr")
            .map_err(|e| e.to_string())?;
        assert_eq!(values, vec![vec![10.0, 20.0, 30.0, 20.0, 30.0, 40.0]]);
        Ok(())
    }
    #[test]
    fn field_edit_is_immediately_reflected_in_config_text() -> Result<(), String> {
        let mut session = session()?;
        session.fields(&[("vfov", "51.25".into()), ("src_fn", "3".into())])?;
        assert_eq!(session.document()?.get("vfov"), Some("51.25"));
        assert_eq!(session.source_edit, "3");
        assert!(session.history.undo());
        assert_eq!(session.document()?.get("vfov"), Some("45"));
        Ok(())
    }
    fn pointer_frame(ctx: &egui::Context, session: &mut Session, events: Vec<egui::Event>) {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(500.0, 400.0))),
            events,
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                session.interact_source(
                    ui,
                    Rect::from_min_size(Pos2::new(20.0, 20.0), Vec2::new(384.0, 288.0)),
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                );
            });
        });
    }
    fn click(
        ctx: &egui::Context,
        session: &mut Session,
        point: [f64; 2],
        button: egui::PointerButton,
    ) {
        let pos = Pos2::new(point[0] as f32 + 20.0, point[1] as f32 + 20.0);
        pointer_frame(
            ctx,
            session,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        pointer_frame(
            ctx,
            session,
            vec![egui::Event::PointerButton {
                pos,
                button,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
    }
    #[test]
    fn real_pointer_actions_select_only_active_geometry_and_delete_nearest() -> Result<(), String> {
        let mut session = session()?;
        let ctx = egui::Context::default();
        pointer_frame(&ctx, &mut session, vec![]);
        let original = session.history.text.clone();
        session.tool = Tool::Rim;
        click(
            &ctx,
            &mut session,
            [120.0, 100.0],
            egui::PointerButton::Primary,
        );
        click(
            &ctx,
            &mut session,
            [140.0, 110.0],
            egui::PointerButton::Primary,
        );
        assert_eq!(session.points, vec![[120.0, 100.0], [140.0, 110.0]]);
        click(
            &ctx,
            &mut session,
            [121.0, 100.0],
            egui::PointerButton::Secondary,
        );
        assert_eq!(session.points, vec![[140.0, 110.0]]);
        assert_eq!(session.history.text, original);
        session.tool = Tool::Inspect;
        session.points.clear();
        click(
            &ctx,
            &mut session,
            [160.0, 110.0],
            egui::PointerButton::Primary,
        );
        assert!(session.points.is_empty());
        session.tool = Tool::Exclude;
        let before: Vec<Vec<f64>> = session
            .document()?
            .vector("roi_ignr")
            .map_err(|e| e.to_string())?;
        click(
            &ctx,
            &mut session,
            [96.0, 156.0],
            egui::PointerButton::Secondary,
        );
        let after: Vec<Vec<f64>> = session
            .document()?
            .vector("roi_ignr")
            .map_err(|e| e.to_string())?;
        assert_eq!(after[0].len(), before[0].len() - 2);
        assert_eq!(after[1], before[1]);
        assert!(session.history.undo());
        assert_eq!(session.history.text, original);
        Ok(())
    }
    #[test]
    fn square_corner_replacement_keeps_legacy_corner_order() -> Result<(), String> {
        let mut session = session()?;
        let ctx = egui::Context::default();
        session.tool = Tool::Square;
        pointer_frame(&ctx, &mut session, vec![]);
        let before: Vec<f64> = session
            .document()?
            .vector("c2a_cnrs_xy")
            .map_err(|e| e.to_string())?;
        click(
            &ctx,
            &mut session,
            [191.0, 171.0],
            egui::PointerButton::Secondary,
        );
        assert_eq!(session.points.len(), 3);
        click(
            &ctx,
            &mut session,
            [191.0, 171.0],
            egui::PointerButton::Primary,
        );
        let after: Vec<f64> = session
            .document()?
            .vector("c2a_cnrs_xy")
            .map_err(|e| e.to_string())?;
        assert_eq!(before, after);
        assert!(session.square_slots.is_none());
        assert!(session.points.is_empty());
        Ok(())
    }
    #[test]
    fn display_maps_non_square_pixels_uniformly_in_full_and_cropped_views() {
        let bounds = Rect::from_min_size(Pos2::new(8.0, 16.0), Vec2::new(500.0, 280.0));
        for (w, h, uv) in [
            (
                1920,
                1080,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            ),
            (
                384,
                288,
                Rect::from_min_max(Pos2::new(0.1, 0.2), Pos2::new(0.7, 0.8)),
            ),
            (
                1080,
                1920,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            ),
        ] {
            let rect = fit_image(bounds, w, h, uv);
            let sx = rect.width() / (w as f32 * uv.width());
            let sy = rect.height() / (h as f32 * uv.height());
            assert!((sx - sy).abs() < 1e-6);
            assert_eq!(rect.center(), bounds.center());
            assert!(
                rect.width() <= bounds.width() + 1e-4 && rect.height() <= bounds.height() + 1e-4
            );
        }
    }
}
