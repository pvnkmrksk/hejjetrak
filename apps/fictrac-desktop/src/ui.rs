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
#[derive(Clone, Copy)]
struct PreviewChrome {
    toolbar: bool,
    status: bool,
}
fn preview_chrome(size: Vec2) -> PreviewChrome {
    PreviewChrome {
        toolbar: size.x >= 160.0 && size.y >= 120.0,
        status: size.x >= 320.0 && size.y >= 220.0,
    }
}
impl Session {
    pub fn show(&mut self, ctx: &egui::Context) {
        self.update();
        let size = ctx.content_rect().size();
        if self.tool != Tool::Inspect
            && self.calibration_preview_size.is_none()
            && (size.x < 480.0 || size.y < 360.0)
        {
            self.calibration_preview_size = Some(size);
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(640.0, 480.0)));
        } else if self.tool == Tool::Inspect
            && let Some(size) = self.calibration_preview_size.take()
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
        }
        self.window_rect = ctx
            .input(|i| i.viewport().outer_rect)
            .map(|r| [r.min.x, r.min.y, r.width(), r.height()]);
        let title = self.window_title();
        if self.last_title != title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }
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
                && !self.calibration_review
                && !self.calibration_redraw
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
        let chrome = preview_chrome(ctx.content_rect().size());
        if chrome.toolbar {
            egui::TopBottomPanel::top(egui::Id::new(("toolbar", self.id)))
                .frame(
                    egui::Frame::new()
                        .fill(ctx.style().visuals.panel_fill)
                        .inner_margin(5.0),
                )
                .show(ctx, |ui| {
                    self.toolbar(ui);
                });
        }
        if chrome.status {
            egui::TopBottomPanel::bottom(egui::Id::new(("status", self.id))).show(ctx, |ui| {
                self.status_bar(ui);
            });
        }
        self.refresh_textures(ctx);
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(Color32::from_rgb(12, 15, 19)))
            .show(ctx, |ui| {
                let rect = ui.available_rect_before_wrap();
                if self.show_diagnostics && chrome.toolbar {
                    let width = rect.width().min(rect.height() * 4.0 / 3.0);
                    let size = Vec2::new(width, width * 0.75);
                    self.canvas(ui, Rect::from_center_size(rect.center(), size));
                } else {
                    self.preview(ui, rect);
                }
            });
        if self.show_settings {
            let mut close = false;
            let layout = ctx.input(|i| {
                i.viewport()
                    .outer_rect
                    .zip(i.viewport().monitor_size)
                    .and_then(|(window, monitor)| calibration::inspector_layout(window, monitor))
            });
            let inner = layout
                .map(|(_, inner)| inner)
                .unwrap_or(egui::vec2(440.0, 640.0));
            let mut builder = egui::ViewportBuilder::default()
                .with_title(format!("hejjetrak · {} configuration", self.display_name()))
                .with_inner_size(inner)
                .with_min_inner_size([340.0_f32.min(inner.x), 360.0_f32.min(inner.y)]);
            if let Some((reserved, _)) = layout {
                builder = builder.with_position(reserved.min);
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
                            && !self.calibration_review
                            && !self.calibration_redraw
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
        ctx.request_repaint_after(std::time::Duration::from_millis(
            if self.snapshot.paused || self.snapshot.ended || self.worker.is_none() {
                250
            } else {
                30
            },
        ));
    }
    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| self.toolbar_row(ui));
    }
    fn toolbar_row(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().item_spacing.x = 3.0;
        let width = ui.available_width();
        if width > 620.0 {
            ui.strong(self.display_name());
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
        if width < 270.0 {
            return;
        }
        if width > 400.0 {
            let record_menu = icons::menu(ui, Icon::ChevronDown, "Recording options", |ui| {
                self.recording_menu(ui)
            })
            .response;
            record_menu.on_hover_text("Choose raw video, processed video, both, or data only");
            icons::menu(ui, Icon::Camera, "Select camera or video", |ui| {
                self.source_controls(ui)
            })
            .response
            .on_hover_text("Select a camera or video; playback controls");
        }
        if icons::button(
            ui,
            Icon::Crop,
            !self.full_image,
            "Switch full source / cropped ball view; preserve pixel aspect ratio",
        )
        .clicked()
        {
            self.full_image = !self.full_image;
        }
        if width > 400.0
            && icons::button(
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
            "Edit configuration and calibrate one step at a time",
        )
        .clicked()
        {
            self.show_settings = !self.show_settings;
        }
        icons::menu(ui, Icon::More, "More actions", |ui| {
            if width <= 400.0 {
                ui.menu_button("Source", |ui| self.source_controls(ui));
                ui.menu_button("Recording mode", |ui| self.recording_menu(ui));
                ui.checkbox(&mut self.show_diagnostics, "Diagnostic panels");
            }
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
            "{:.1} fps · {:.2} ms track{}",
            self.snapshot.actual_fps,
            self.snapshot.tracking_ms,
            if self.snapshot.dropped_running > 0 {
                format!(" · drop {}", self.snapshot.dropped_running)
            } else {
                String::new()
            }
        );
        let detail = format!(
            "{} frames\nTracking mean: {:.3} ms ({} measured frames; ROI + fitting + integration + numerical maps)\nWorker: {:.2} ms (conversion + tracking + diagnostics + recording + dispatch; excludes camera wait and playback pacing)\nSource: {:.2} fps · worker headroom {:.0}%\nCapture frames skipped: {} (includes intentional pauses)\n{}\nUDP {} · ZMQ {}\n{}\n{}",
            self.snapshot.frames,
            self.snapshot.tracking_ms,
            self.snapshot.tracking_samples,
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
            ui.label(egui::RichText::new(text).color(color))
                .on_hover_text(detail);
            if self.snapshot.paused {
                ui.label("Paused");
            } else if self.snapshot.ended {
                ui.label("Done");
            } else if !quality && self.snapshot.row.is_some() {
                ui.label("Lost").on_hover_text(
                    "Motion is unmeasured while tracking is unreliable. Inspect calibration if loss persists.",
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
        ui.heading("Calibration");
        if self.tool == Tool::Inspect {
            ui.label("Review the saved setup one step at a time. Keep what is correct, edit a mark, or redraw just that step.");
            if ui.button("Calibrate again…").on_hover_text("Pause this source and begin with the saved ball outline. No configuration changes until an edit is applied.").clicked() {
                self.tool = Tool::Rim; self.begin_calibration_step();
            }
            ui.checkbox(&mut self.show_overlays, "Inspect all saved outlines")
                .on_hover_text(
                    "Show the current rim, exclusions and lab axes without editing them.",
                );
        } else {
            let before = self.tool;
            ui.horizontal_wrapped(|ui| self.calibration_tools(ui));
            if before != self.tool {
                if !self.points.is_empty() {
                    self.tool = before;
                    self.error =
                        "Apply or cancel the current draft before changing calibration steps"
                            .into();
                } else {
                    self.begin_calibration_step();
                }
            }
            ui.separator();
            ui.strong(match self.tool {
                Tool::Rim => "1 of 3 · Ball outline",
                Tool::Exclude => "2 of 3 · Excluded regions",
                Tool::Square => "3 of 3 · Lab orientation",
                _ => "Saved calibration",
            });
            ui.label(self.saved_calibration_summary());
            if self.calibration_review {
                ui.small("Inspect the highlighted geometry in the paused preview. This step is read-only until you choose Edit or Redraw.");
                ui.horizontal_wrapped(|ui| {
                    if ui.button(if self.tool == Tool::Square { "Keep & finish" } else { "Keep & continue" }).on_hover_text("Preserve this step exactly and proceed; no config fields are rewritten.").clicked() { let result = self.advance_calibration_step(); self.report(result); }
                    if ui.add_enabled(self.saved_mark_count() > 0, egui::Button::new("Edit saved marks")).on_hover_text("Drag or right-click saved marks for this step only. If no clicks were saved, redraw them instead.").clicked() { let result = self.choose_calibration_action("edit"); self.report(result); }
                    if ui.button("Redraw this step").on_hover_text("Start an isolated replacement draft. Other steps and the saved file stay intact until a valid replacement is applied.").clicked() { let result = self.choose_calibration_action("redraw"); self.report(result); }
                });
                if self.tool == Tool::Exclude
                    && ui
                        .button("Add another region")
                        .on_hover_text("Keep existing exclusions and append one new polygon.")
                        .clicked()
                {
                    let result = self.choose_calibration_action("add");
                    self.report(result);
                }
            } else {
                if self.edit_existing {
                    ui.label("Drag a saved mark to move it. Right-click a mark to remove it. Invalid edits leave the saved geometry intact.");
                    if ui.button("Continue with these edits").clicked() {
                        let result = self.advance_calibration_step();
                        self.report(result);
                    }
                } else {
                    ui.label(match self.tool {
                        Tool::Rim => {
                            "Click at least three well-separated points on the ball boundary."
                        }
                        Tool::Exclude => {
                            "Click around the region to ignore. Enter closes this polygon."
                        }
                        Tool::Square => "Click square corners in order: TL → TR → BR → BL.",
                        _ => "Choose a calibration step.",
                    });
                    ui.small(if self.calibration_redraw { "Replacement draft: saved marks are hidden here and remain in the config until you apply a valid replacement." } else { "New region: existing exclusions remain unchanged." });
                    ui.horizontal_wrapped(|ui| {
                        if ui.add_enabled(!self.points.is_empty(), egui::Button::new("Apply draft")).on_hover_text("Validate and replace only this step in one undoable edit.").clicked() { let result = self.finish_points(); self.report(result); }
                        if ui.button("Clear draft points").clicked() { self.points.clear(); self.square_slots = None; }
                        if self.tool == Tool::Exclude && self.calibration_redraw && self.points.is_empty() && ui.button("Use no exclusions").on_hover_text("Remove all exclusion polygons as one undoable edit; keep the ball and lab calibration.").clicked() {
                            let result = self.commit_polygons(&[]).and_then(|_| self.apply()); if result.is_ok() { self.calibration_redraw = false; self.calibration_review = true; } self.report(result);
                        }
                    });
                }
                if ui
                    .button("Back to saved review / cancel draft")
                    .on_hover_text(
                        "Discard uncommitted draft points. Applied edits remain available in Undo.",
                    )
                    .clicked()
                {
                    let result = self.choose_calibration_action("inspect");
                    self.report(result);
                }
            }
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut self.magnifier, "Precision magnifier").on_hover_text("A large nearest-pixel view magnifies the current preview fourfold, with saved outlines and numbered marks. It uses a separate native window when space permits.");
                ui.checkbox(&mut self.full_image, "Full image").on_hover_text("Switch between the whole scene and an undistorted close-up of the ball.");
            });
            if self.tool == Tool::Square {
                ui.horizontal(|ui| {
                    ui.label("Lab plane");
                    for plane in ["xy", "yz", "xz"] {
                        ui.selectable_value(&mut self.plane, plane.into(), plane.to_uppercase());
                    }
                });
                if ui.add_enabled(!self.calibration_review && !self.calibration_redraw, egui::Button::new("Alternate fitted orientation  F")).on_hover_text("Choose Edit saved marks to refit the alternate pose branch without changing corner order.").clicked() { let result = self.cycle_square(); self.report(result); }
            }
            if ui.button("Finish calibration / resume").on_hover_text("Apply any valid current draft and resume tracking. Save configuration preserves changes across launches.").clicked() {
                let result = self.complete_calibration(); self.report(result);
            }
        }
        ui.collapsing("Resolution reference", |ui| {
            ui.small("FicTrac rim/exclusion/square marks are pixels; roi_c/roi_r and fitted orientation are angular. Opt in below before a same-aspect, same-field-of-view resolution change. Optical zoom, crops or aspect changes require calibration again.");
            if let Some(image) = self.snapshot.image.clone() {
                let size = format!("{} × {}", image.width, image.height);
                if ui.button(format!("Use {size} as calibration reference")).on_hover_text("Adds normalized Rust extension metadata without changing legacy fields. Confirm that saved marks match this frame first.").clicked() {
                    let result = self.document().and_then(|doc| fictrac_io::capture_calibration_reference(&doc, image.width, image.height).map_err(|e| e.to_string())).map(|doc| self.edit(doc.text().to_owned())); self.report(result);
                }
                if let Ok(doc) = self.document() && let Some(reference) = doc.get("rust_calibration_size") {
                    ui.small(format!("Saved reference: {reference} pixels"));
                    if ui.button(format!("Adapt saved marks to {size}")).on_hover_text("Explicitly scales pixel marks from the saved normalized reference. Angular geometry stays unchanged; aspect changes or stale metadata are rejected.").clicked() {
                        let result = fictrac_io::rescale_calibration_reference(&doc, image.width, image.height).map_err(|e| e.to_string()).map(|next| self.edit(next.text().to_owned())); self.report(result);
                    }
                }
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
            (
                Tool::Rim,
                "1 · Ball",
                "Fit the sphere outline with well-separated rim points",
            ),
            (
                Tool::Exclude,
                "2 · Exclusions",
                "Draw optional regions to ignore",
            ),
            (
                Tool::Square,
                "3 · Lab",
                "Set forward axes using four ordered square corners",
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
            if icons::button(
                ui,
                if self.paused { Icon::Play } else { Icon::Pause },
                false,
                if self.paused {
                    "Resume source playback"
                } else {
                    "Pause source playback"
                },
            )
            .clicked()
            {
                self.paused = !self.paused;
                self.send(Command::Pause(self.paused));
            }
            if icons::button(ui, Icon::Step, false, "Process one frame").clicked() {
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
            .default_open(true)
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
                        if category == "Tracking & quality" {
                            let mut resilient = self.document().map(|doc| doc.get("rust_resilient").is_none() || doc.boolean("rust_resilient")).unwrap_or(true);
                            if ui.checkbox(&mut resilient, "Preserve atlas and recover after occlusion").on_hover_text("Protect learned ball appearance during obscured frames and retain it for bounded reacquisition. Missing motion remains unmeasured; disable to use the legacy reset policy.").changed() {
                                changes.push(("rust_resilient", if resilient { "y" } else { "n" }.into()));
                            }
                        }
                        egui::Grid::new(("fields", self.id, category))
                            .num_columns(2)
                            .spacing([12.0, 6.0])
                            .show(ui, |ui| {
                                for (key, original) in fields
                                    .iter()
                                    .filter(|(key, _)| field_category(key) == category && key != "rust_resilient" && !key.starts_with("rust_normalized_") && !key.starts_with("rust_calibration_"))
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
        if let Some(crop) = self
            .calibration_cache
            .borrow()
            .as_ref()
            .and_then(|cache| cache.crop)
        {
            return crop;
        }
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
        let crop = if max.x <= min.x || max.y <= min.y {
            full
        } else {
            Rect::from_min_max(min, max)
        };
        if let Some(cache) = self.calibration_cache.borrow_mut().as_mut() {
            cache.crop = Some(crop);
        }
        crop
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
        if self.tool == Tool::Inspect && !self.snapshot.path.is_empty() && rect.width() > 300.0 {
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
        let precision = self.tool != Tool::Inspect && self.magnifier;
        let key = u64::from(self.show_diagnostics) | (u64::from(precision) << 1);
        let Some(frame) = &self.snapshot.image else {
            return;
        };
        if self.texture_frame == key
            && self
                .texture_image
                .as_ref()
                .is_some_and(|previous| std::sync::Arc::ptr_eq(previous, frame))
        {
            return;
        }
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
            let options = if i == 0 && precision {
                TextureOptions::NEAREST
            } else {
                TextureOptions::LINEAR
            };
            if let Some(texture) = self.textures.get_mut(i) {
                texture.set(image, options);
            } else {
                self.textures.push(ctx.load_texture(
                    format!("session-{}-{i}", self.id),
                    image,
                    options,
                ));
            }
        }
        self.texture_frame = key;
        self.texture_image = Some(frame.clone());
    }
    pub(crate) fn core_config(&self) -> Result<Config, String> {
        let (width, height) = self
            .snapshot
            .image
            .as_ref()
            .map(|i| (i.width, i.height))
            .unwrap_or((384, 288));
        if let Some(cache) = self.calibration_cache.borrow().as_ref()
            && cache.text == self.history.text
            && cache.width == width
            && cache.height == height
        {
            return cache.config.clone();
        }
        let config = self.document().and_then(|mut doc| {
            doc.set("src_fps", "-1").map_err(|e| e.to_string())?;
            load_config_text(doc.text(), width, height)
                .map(|l| l.config)
                .map_err(|e| e.to_string())
        });
        *self.calibration_cache.borrow_mut() = Some(crate::session::CalibrationCache {
            text: self.history.text.clone(),
            width,
            height,
            config: config.clone(),
            crop: None,
        });
        config
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
        if self.tool == Tool::Inspect {
            response.context_menu(|ui| {
                if ui.button("Reset tracking").clicked() {
                    self.send(Command::Reset);
                    ui.close();
                }
                if ui
                    .button(if self.snapshot.recording {
                        "Stop recording"
                    } else {
                        "Start recording"
                    })
                    .clicked()
                {
                    if self.snapshot.recording {
                        self.send(Command::StopRecord);
                    } else {
                        self.begin_recording();
                    }
                    ui.close();
                }
                if ui.button("Settings…").clicked() {
                    self.show_settings = true;
                    ui.close();
                }
                if ui.button("Calibrate again…").clicked() {
                    self.show_settings = true;
                    self.tool = Tool::Rim;
                    self.begin_calibration_step();
                    ui.close();
                }
            });
        }
        let painter = ui.painter_at(rect);
        let saved_visible = !self.calibration_redraw;
        let mut rim = vec![];
        let mut polygons = vec![];
        let mut square = vec![];
        let mut square_plane = "xy".to_owned();
        if (self.show_overlays || self.tool != Tool::Inspect)
            && let Ok(doc) = self.document()
        {
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
        if (self.show_overlays
            || self.tool != Tool::Inspect
            || !self.snapshot.contact_trace.is_empty())
            && let Ok(config) = self.core_config()
        {
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
                .filter(|_| {
                    saved_visible
                        && (self.show_overlays || matches!(self.tool, Tool::Rim | Tool::Edit))
                })
                .filter_map(|i| {
                    let a = i as f64 * std::f64::consts::TAU / 100.0;
                    let ray = centre * radius.cos() + (u * a.cos() + v * a.sin()) * radius.sin();
                    geometry
                        .as_ref()
                        .and_then(|g| g.project_ray(ray))
                        .map(to_screen)
                })
                .collect();
            if saved_visible && (self.show_overlays || matches!(self.tool, Tool::Rim | Tool::Edit))
            {
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
            if self.tool == Tool::Inspect
                && let (Some(row), Some(geometry)) = (&self.snapshot.row, geometry.as_ref())
            {
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
            if saved_visible
                && (self.show_overlays || matches!(self.tool, Tool::Square | Tool::Edit))
            {
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
        if saved_visible && (self.show_overlays || matches!(self.tool, Tool::Exclude | Tool::Edit))
        {
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
            if matches!(self.tool, Tool::Rim | Tool::Edit) {
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
        if saved_visible && self.tool == Tool::Rim {
            for p in &rim {
                painter.circle_filled(to_screen(*p), 3.5, Color32::LIGHT_GREEN);
            }
        }
        if saved_visible
            && (self.show_overlays || self.tool == Tool::Square || self.tool == Tool::Edit)
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
        if self.tool == Tool::Square
            && let Some(slots) = self.square_slots
        {
            for i in 0..4 {
                if let (Some(a), Some(b)) = (slots[i], slots[(i + 1) % 4]) {
                    painter
                        .line_segment([to_screen(a), to_screen(b)], Stroke::new(2.0_f32, YELLOW));
                }
            }
        } else if preview.len() > 1 {
            painter.add(egui::Shape::line(
                preview.clone(),
                Stroke::new(2.0_f32, YELLOW),
            ));
        }
        for p in preview {
            painter.circle_filled(p, 4.0, YELLOW);
        }
        if self.tool != Tool::Inspect
            && self.magnifier
            && let Some(pos) = response.hover_pos()
        {
            let active: Vec<[f64; 2]> = if self.calibration_redraw {
                vec![]
            } else {
                match self.tool {
                    Tool::Rim => rim.clone(),
                    Tool::Exclude => polygons.iter().flatten().copied().collect(),
                    Tool::Square => square.clone(),
                    _ => vec![],
                }
            };
            let display_scale = rect.width() / (w as f32 * uv.width());
            self.draw_magnifier(ui, rect, pos, (to_source(pos), display_scale), &active);
        }
        if self.calibration_review && self.tool != Tool::Edit {
            return;
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
            if self.calibration_redraw {
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
                    self.edit_existing = false;
                    self.calibration_review = false;
                    self.calibration_redraw = true;
                    self.status = "Replace the missing corner; saved lab calibration is retained until a complete fit".into();
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
                        self.edit_existing = false;
                        self.calibration_review = false;
                        self.calibration_redraw = true;
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
        if self.tool == Tool::Edit || self.edit_existing {
            if response.drag_started()
                && let Some(pos) = response.interact_pointer_pos()
            {
                let mut best = 12.0;
                self.pending_drag = None;
                for (i, p) in rim
                    .iter()
                    .enumerate()
                    .filter(|_| matches!(self.tool, Tool::Rim | Tool::Edit))
                {
                    let d = to_screen(*p).distance(pos);
                    if d < best {
                        best = d;
                        self.pending_drag = Some((0, i));
                    }
                }
                for (i, polygon) in polygons
                    .iter()
                    .enumerate()
                    .filter(|_| matches!(self.tool, Tool::Exclude | Tool::Edit))
                {
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
                && matches!(self.tool, Tool::Square | Tool::Edit)
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
    fn draw_magnifier(
        &self,
        ui: &egui::Ui,
        bounds: Rect,
        cursor: Pos2,
        sample: ([f64; 2], f32),
        saved: &[[f64; 2]],
    ) {
        let (source, display_scale) = sample;
        let (Some(image), Some(texture)) = (&self.snapshot.image, self.textures.first()) else {
            return;
        };
        let (external, inspector_inset) = ui.ctx().input(|i| {
            let viewport = i.viewport();
            viewport
                .outer_rect
                .zip(viewport.monitor_size)
                .map(|(window, monitor)| {
                    let inspector = self
                        .show_settings
                        .then(|| calibration::inspector_layout(window, monitor).map(|(r, _)| r))
                        .flatten();
                    let local_inspector = inspector
                        .map(|r| r.translate(-viewport.inner_rect.unwrap_or(window).min.to_vec2()));
                    (
                        calibration::magnifier_position(
                            window,
                            monitor,
                            egui::vec2(240.0, 268.0),
                            inspector,
                        ),
                        local_inspector,
                    )
                })
                .unwrap_or((None, None))
        });
        let size = if external.is_some() {
            240.0
        } else {
            240.0_f32
                .min(bounds.width() - 12.0)
                .min(bounds.height() - 36.0)
        };
        if size < 120.0 {
            return;
        }
        let inset = if external.is_none() {
            calibration::magnifier_inset(
                bounds,
                cursor,
                egui::vec2(size, size + 24.0),
                inspector_inset,
            )
        } else {
            None
        };
        if external.is_none() && inset.is_none() {
            return;
        }
        let side = (size / (4.0 * display_scale.max(0.0001)))
            .clamp(1.0, image.width.min(image.height) as f32);
        let origin = [
            (source[0] - f64::from(side) * 0.5).clamp(0.0, image.width as f64 - f64::from(side)),
            (source[1] - f64::from(side) * 0.5).clamp(0.0, image.height as f64 - f64::from(side)),
        ];
        let uv = Rect::from_min_max(
            egui::pos2(
                (origin[0] / image.width as f64) as f32,
                (origin[1] / image.height as f64) as f32,
            ),
            egui::pos2(
                ((origin[0] + f64::from(side)) / image.width as f64) as f32,
                ((origin[1] + f64::from(side)) / image.height as f64) as f32,
            ),
        );
        let lines = self.calibration_lines();
        let paint = |painter: &egui::Painter, lens: Rect| {
            painter.rect_filled(
                lens.expand2(egui::vec2(2.0, 24.0)),
                3.0,
                Color32::from_rgb(10, 13, 18),
            );
            painter.image(texture.id(), lens, uv, Color32::WHITE);
            let to_lens = |p: [f64; 2]| {
                egui::pos2(
                    lens.left() + ((p[0] - origin[0]) / f64::from(side)) as f32 * size,
                    lens.top() + ((p[1] - origin[1]) / f64::from(side)) as f32 * size,
                )
            };
            let clipped = painter.with_clip_rect(lens);
            for line in &lines {
                let points: Vec<_> = line.iter().map(|p| to_lens(*p)).collect();
                clipped.add(egui::Shape::line(
                    points.clone(),
                    Stroke::new(3.0_f32, Color32::BLACK),
                ));
                clipped.add(egui::Shape::line(
                    points,
                    Stroke::new(1.5_f32, Color32::LIGHT_GREEN),
                ));
            }
            if self.tool == Tool::Square
                && let Some(slots) = self.square_slots
            {
                for i in 0..4 {
                    if let (Some(a), Some(b)) = (slots[i], slots[(i + 1) % 4]) {
                        clipped
                            .line_segment([to_lens(a), to_lens(b)], Stroke::new(2.0_f32, YELLOW));
                    }
                }
            } else if self.points.len() > 1 {
                clipped.add(egui::Shape::line(
                    self.points.iter().map(|p| to_lens(*p)).collect(),
                    Stroke::new(2.0_f32, YELLOW),
                ));
            }
            for (points, color) in [
                (saved, Color32::LIGHT_GREEN),
                (self.points.as_slice(), YELLOW),
            ] {
                for (index, point) in points.iter().enumerate() {
                    let pixel = to_lens(*point);
                    clipped.circle_stroke(pixel, 4.0, Stroke::new(1.5_f32, color));
                    let label_index = if color == YELLOW && self.tool == Tool::Square {
                        self.square_slots
                            .as_ref()
                            .and_then(|slots| {
                                slots
                                    .iter()
                                    .enumerate()
                                    .filter(|(_, p)| p.is_some())
                                    .nth(index)
                                    .map(|(i, _)| i)
                            })
                            .unwrap_or(index)
                    } else {
                        index
                    };
                    let label = if self.tool == Tool::Square {
                        ["TL", "TR", "BR", "BL"]
                            .get(label_index)
                            .copied()
                            .unwrap_or("")
                            .to_owned()
                    } else {
                        (index + 1).to_string()
                    };
                    clipped.text(
                        pixel + egui::vec2(6.0, -6.0),
                        Align2::LEFT_BOTTOM,
                        label,
                        FontId::proportional(12.0),
                        color,
                    );
                }
            }
            let cross = to_lens(source);
            for (a, b) in [
                (egui::vec2(-12.0, 0.0), egui::vec2(12.0, 0.0)),
                (egui::vec2(0.0, -12.0), egui::vec2(0.0, 12.0)),
            ] {
                clipped.line_segment([cross + a, cross + b], Stroke::new(3.0_f32, Color32::BLACK));
                clipped.line_segment([cross + a, cross + b], Stroke::new(1.0_f32, Color32::WHITE));
            }
            painter.text(
                lens.left_bottom() + egui::vec2(3.0, 4.0),
                Align2::LEFT_TOP,
                format!(
                    "{:.0}× view · {:.1}, {:.1} source px",
                    size / (side * display_scale),
                    source[0],
                    source[1]
                ),
                FontId::proportional(12.0),
                Color32::WHITE,
            );
        };
        if let Some(position) = external {
            let builder = egui::ViewportBuilder::default()
                .with_title("Calibration precision")
                .with_position(position)
                .with_inner_size([size, size + 24.0])
                .with_resizable(false)
                .with_decorations(false)
                .with_active(false)
                .with_mouse_passthrough(true)
                .with_always_on_top();
            ui.ctx().show_viewport_immediate(
                egui::ViewportId::from_hash_of(("calibration-magnifier", self.id)),
                builder,
                |ctx, _| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE.fill(Color32::from_rgb(10, 13, 18)))
                        .show(ctx, |ui| {
                            paint(
                                ui.painter(),
                                Rect::from_min_size(ui.min_rect().min, egui::vec2(size, size)),
                            );
                        });
                },
            );
        } else if let Some(inset) = inset {
            paint(
                &ui.painter_at(bounds),
                Rect::from_min_size(inset.min, egui::vec2(size, size)),
            );
        }
    }
    fn commit_rim(&mut self, points: &[[f64; 2]]) -> Result<(), String> {
        let document = self.calibration_document(Tool::Rim, points, &self.plane, false)?;
        self.edit(document.text().to_owned());
        Ok(())
    }
    pub(crate) fn commit_polygons(&mut self, polygons: &[Vec<[f64; 2]>]) -> Result<(), String> {
        let document = fictrac_control::replace_exclusions_document(&self.document()?, polygons)?;
        self.edit(document.text().to_owned());
        Ok(())
    }
    fn commit_square(&mut self, points: &[[f64; 2]], plane: &str) -> Result<(), String> {
        let document = self.calibration_document(Tool::Square, points, plane, false)?;
        self.edit(document.text().to_owned());
        Ok(())
    }
    pub(crate) fn finish_points(&mut self) -> Result<(), String> {
        let document = self
            .calibration_draft_document()?
            .ok_or("Place draft points before applying calibration")?;
        self.edit(document.text().to_owned());
        self.points.clear();
        self.square_slots = None;
        self.calibration_review = true;
        self.calibration_redraw = false;
        self.edit_existing = false;
        self.apply()
    }
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
        // These legacy pointer-unit helpers explicitly exercise point placing.
        // The actual wizard starts in read-only review and is tested below.
        session.calibration_review = false;
        session.snapshot.image = Some(Arc::new(ViewFrame {
            width: 384,
            height: 288,
            rgb: vec![128; 384 * 288 * 3],
            debug: None,
        }));
        Ok(session)
    }
    #[test]
    fn saved_calibration_review_is_read_only_and_keep_advances_without_rewriting()
    -> Result<(), String> {
        let mut session = session()?;
        let original = session.history.text.clone();
        session.tool = Tool::Rim;
        session.begin_calibration_step();
        let ctx = egui::Context::default();
        pointer_frame(&ctx, &mut session, vec![]);
        click(
            &ctx,
            &mut session,
            [63.0, 171.0],
            egui::PointerButton::Secondary,
        );
        click(
            &ctx,
            &mut session,
            [80.0, 120.0],
            egui::PointerButton::Primary,
        );
        assert!(session.points.is_empty());
        assert_eq!(session.history.text, original);
        session.choose_calibration_action("keep")?;
        assert!(session.tool == Tool::Exclude);
        session.choose_calibration_action("keep")?;
        assert!(session.tool == Tool::Square);
        session.choose_calibration_action("keep")?;
        assert!(session.tool == Tool::Inspect);
        assert!(!session.paused);
        assert_eq!(session.history.text, original);
        Ok(())
    }
    #[test]
    fn redraw_replaces_only_current_step_as_one_undoable_edit() -> Result<(), String> {
        let mut session = session()?;
        let original = session.history.text.clone();
        let original_doc = session.document()?;
        session.tool = Tool::Exclude;
        session.begin_calibration_step();
        session.choose_calibration_action("redraw")?;
        assert!(session.calibration_lines().is_empty());
        assert_eq!(session.history.text, original);
        session.points = vec![[10.0, 10.0], [25.0, 10.0], [20.0, 30.0]];
        session.finish_points()?;
        let updated = session.document()?;
        assert_eq!(updated.get("roi_circ"), original_doc.get("roi_circ"));
        assert_eq!(updated.get("c2a_r"), original_doc.get("c2a_r"));
        assert_eq!(
            updated
                .vector::<Vec<Vec<f64>>>("roi_ignr")
                .map_err(|e| e.to_string())?,
            vec![vec![10.0, 10.0, 25.0, 10.0, 20.0, 30.0]]
        );
        assert!(session.history.undo());
        assert_eq!(session.history.text, original);
        session.tool = Tool::Rim;
        session.begin_calibration_step();
        session.choose_calibration_action("redraw")?;
        assert!(session.complete_calibration().is_err());
        assert_eq!(session.history.text, original);
        session.choose_calibration_action("inspect")?;
        session.choose_calibration_action("keep")?;
        session.choose_calibration_action("redraw")?;
        session.choose_calibration_action("keep")?;
        assert!(
            session
                .document()?
                .vector::<Vec<Vec<f64>>>("roi_ignr")
                .map_err(|e| e.to_string())?
                .is_empty()
        );
        assert!(session.history.undo());
        assert_eq!(session.history.text, original);
        Ok(())
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
        pointer_frame_uv(
            ctx,
            session,
            events,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
        );
    }
    fn pointer_frame_uv(
        ctx: &egui::Context,
        session: &mut Session,
        events: Vec<egui::Event>,
        uv: Rect,
    ) {
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
                    uv,
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
        session.edit_existing = true;
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
    fn cropped_calibration_clicks_map_back_to_original_source_pixels() -> Result<(), String> {
        let mut session = session()?;
        session.tool = Tool::Rim;
        let ctx = egui::Context::default();
        let uv = Rect::from_min_max(egui::pos2(0.25, 0.25), egui::pos2(0.75, 0.75));
        pointer_frame_uv(&ctx, &mut session, vec![], uv);
        let pos = egui::pos2(212.0, 164.0);
        for pressed in [true, false] {
            pointer_frame_uv(
                &ctx,
                &mut session,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                uv,
            );
        }
        assert_eq!(session.points, vec![[192.0, 144.0]]);
        Ok(())
    }
    #[test]
    fn edit_mode_cannot_add_points_or_drag_unselected_geometry() -> Result<(), String> {
        let mut session = session()?;
        session.tool = Tool::Exclude;
        session.edit_existing = true;
        let original = session.history.text.clone();
        let ctx = egui::Context::default();
        pointer_frame(&ctx, &mut session, vec![]);
        click(
            &ctx,
            &mut session,
            [220.0, 100.0],
            egui::PointerButton::Primary,
        );
        assert!(session.points.is_empty());
        let start = egui::pos2(83.0, 191.0); // saved rim point (63,171), outside exclusion marks
        pointer_frame(
            &ctx,
            &mut session,
            vec![
                egui::Event::PointerMoved(start),
                egui::Event::PointerButton {
                    pos: start,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        pointer_frame(
            &ctx,
            &mut session,
            vec![egui::Event::PointerMoved(start + egui::vec2(25.0, 10.0))],
        );
        assert!(session.pending_drag.is_none());
        assert_eq!(session.history.text, original);
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
#[test]
fn tiny_preview_hides_chrome_without_reducing_font_size() {
    let tiny = preview_chrome(egui::vec2(96.0, 96.0));
    assert!(!tiny.toolbar && !tiny.status);
    let compact = preview_chrome(egui::vec2(180.0, 140.0));
    assert!(compact.toolbar && !compact.status);
    let normal = preview_chrome(egui::vec2(480.0, 360.0));
    assert!(normal.toolbar && normal.status);
}
