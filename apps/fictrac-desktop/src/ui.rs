use crate::{
    calibration,
    runtime::Command,
    session::{Session, Tool},
};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Stroke, TextureOptions, Vec2};
use fictrac_core::Config;
use fictrac_io::{load_config_text, parameter_description};
use glam::{DQuat, DVec3};
const YELLOW: Color32 = Color32::from_rgb(230, 235, 35);
macro_rules! act {
    ($s:ident,$expr:expr) => {{
        let result = $expr;
        $s.report(result);
    }};
}
impl Session {
    pub fn show(&mut self, ctx: &egui::Context) {
        self.update();
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
                ui.label("Save these calibration/config edits before closing?");
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
            act!(self, self.save(false));
        }
        if !ctx.wants_keyboard_input() {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::R)) {
                self.send(Command::Reset);
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
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
        let dropped: Vec<_> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        if let Some(path) = dropped.first() {
            if path.extension().is_some_and(|e| e == "txt") {
                act!(self, self.load(path.clone()));
            } else {
                act!(self, self.video(path.clone()));
            }
        }
        egui::TopBottomPanel::top(egui::Id::new(("toolbar", self.id))).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.strong("FicTrac");
                ui.label("Rust · native");
                ui.separator();
                if ui.button("Open config…").clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .add_filter("FicTrac", &["txt"])
                        .pick_file()
                {
                    act!(self, self.load(path));
                }
                if ui.button("Open video…").clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .add_filter("Video", &["mp4", "mov", "y4m"])
                        .pick_file()
                {
                    act!(self, self.video(path));
                }
                if ui
                    .button("Save")
                    .on_hover_text("Save the exact config text (⌘/Ctrl+S)")
                    .clicked()
                {
                    act!(self, self.save(false));
                }
                if ui.button("Save as…").clicked() {
                    act!(self, self.save(true));
                }
                ui.separator();
                if ui
                    .button("New window")
                    .on_hover_text(
                        "Next BR/VR config, or copy this config; numeric camera source increments",
                    )
                    .clicked()
                {
                    self.spawn_request = 1;
                }
                if ui.button("Open four").clicked() {
                    self.spawn_request = 3;
                }
            });
            ui.horizontal(|ui| {
                let title = self
                    .path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "Unsaved independent configuration".into());
                ui.label(format!(
                    "{title}{}",
                    if self.history.text != self.saved {
                        " *"
                    } else {
                        ""
                    }
                ));
                if !self.status.is_empty() {
                    ui.label(
                        egui::RichText::new(&self.status)
                            .small()
                            .color(Color32::LIGHT_GREEN),
                    );
                }
            });
        });
        egui::SidePanel::left(egui::Id::new(("parameters",self.id))).default_width(365.0).min_width(310.0).resizable(true).show(ctx,|ui| {
            ui.horizontal(|ui|{
                if ui.button(if self.paused{"▶ Play"}else{"Ⅱ Pause"}).clicked(){self.paused = !self.paused;self.send(Command::Pause(self.paused));}
                if ui.button("Step").clicked(){self.paused=true;self.send(Command::Step);}
                if ui.button("Replay / Connect").clicked(){act!(self,self.start());}
            });
            ui.horizontal(|ui|{
                if ui.checkbox(&mut self.realtime,"Video at source speed").changed(){self.send(Command::Realtime(self.realtime));}
                if ui.button("Reset map / pose").clicked(){self.send(Command::Reset);}
            });
            ui.separator();
            if let Ok(doc)=self.document(){
                let mut mode=doc.get("rust_engine").unwrap_or("spherical").to_owned();let old=mode.clone();
                egui::ComboBox::from_id_salt(("engine",self.id)).selected_text(if mode=="spherical"{"FicTrac spherical map"}else{"Photometric alternate"}).show_ui(ui,|ui|{
                    ui.selectable_value(&mut mode,"pairwise".into(),"Photometric alternate");
                    ui.selectable_value(&mut mode,"spherical".into(),"FicTrac spherical map BOBYQA");
                });
                if mode!=old{act!(self,self.fields(&[("rust_engine",mode)]));}
            }
            ui.strong("Source");
            ui.horizontal(|ui|{
                let response=ui.add(egui::TextEdit::singleline(&mut self.source_edit).desired_width(215.0).hint_text("camera ID or video path"));
                if response.changed(){act!(self,self.fields(&[("src_fn",self.source_edit.clone())]));}
                if ui.button("Connect").clicked(){act!(self,self.fields(&[("src_fn",self.source_edit.clone())]));act!(self,self.start());}
            });
            ui.horizontal(|ui|{
                if ui.button("List cameras").clicked(){match fictrac_camera::list_cameras(){Ok(c)=>self.status=if c.is_empty(){"No camera devices detected".into()}else{c.iter().map(|c|format!("{}: {}",c.id,c.name)).collect::<Vec<_>>().join(" · ")},Err(e)=>self.error=e.to_string()}}
                if ui.button("Stop source").clicked(){self.worker=None;self.status="Source stopped".into();}
            });
            ui.separator();ui.strong("Graphical calibration");
            let before=self.tool;
            ui.horizontal_wrapped(|ui|{
                for (tool,label) in [(Tool::Inspect,"Inspect"),(Tool::Rim,"Ball rim"),(Tool::Exclude,"Exclude"),(Tool::Forward,"Forward"),(Tool::Square,"Lab square"),(Tool::Edit,"Move points")] {
                    ui.selectable_value(&mut self.tool,tool,label);
                }
            });
            if self.tool!=before{self.points.clear();self.full_image=true;self.paused=true;self.send(Command::Pause(true));}
            ui.checkbox(&mut self.full_image,"Show full source image");
            match self.tool {
                Tool::Rim=>{ui.label("Click 3+ points on the ball rim, then Fit. Coordinates stay in source pixels.");},
                Tool::Exclude=>{ui.label("Click polygon vertices. Finish with Enter or double-click.");},
                Tool::Forward=>{ui.label("Drag the forward direction on the image. Existing down axis is retained; square calibration determines depth.");},
                Tool::Square=>{ui.label("Click square TL → TR → BR → BL in the selected lab plane.");ui.horizontal(|ui|{for plane in ["xy","yz","xz"]{ui.selectable_value(&mut self.plane,plane.into(),plane.to_uppercase());}});},
                Tool::Edit=>{ui.label("Drag a rim or exclusion vertex; the file representation updates with it.");},
                _=>{ui.label("Drop a video here to start. Calibrate against a paused frame.");},
            }
            ui.horizontal(|ui|{
                if ui.button("Finish / Fit").clicked(){act!(self,self.finish_points());}
                if ui.button("Cancel points").clicked(){self.points.clear();}
                if ui.button("Undo").clicked()&&self.history.undo(){self.changed=Some(std::time::Instant::now());}
                if ui.button("Redo").clicked()&&self.history.redo(){self.changed=Some(std::time::Instant::now());}
            });
            if ui.button("Remove last exclusion").clicked(){act!(self,self.remove_exclusion());}
            ui.separator();
            ui.horizontal(|ui|{
                ui.selectable_value(&mut self.raw_tab,false,"Parameters");ui.selectable_value(&mut self.raw_tab,true,"Config text");
                if ui.button("Apply").clicked(){act!(self,self.apply());}
            });
            ui.checkbox(&mut self.auto_apply,"Apply edits automatically (resets tracking epoch)");
            if self.disk_conflict {
                ui.colored_label(Color32::YELLOW,"External file changed while there are unsaved edits.");
                if ui.button("Reload disk version").clicked()&&let Some(path)=self.path.clone(){act!(self,self.load(path));}
            }
            if self.raw_tab {
                let mut text=self.history.text.clone();
                egui::ScrollArea::both().id_salt("raw_config").max_height(ui.available_height()-95.0).show(ui,|ui|{
                    if ui.add(egui::TextEdit::multiline(&mut text).code_editor().desired_width(900.0).desired_rows(27)).changed(){self.edit(text.clone());}
                });
            }else{
                let fields=self.document().map(|d|d.fields().map(|(k,v)|(k.to_owned(),v.to_owned())).collect::<Vec<_>>());
                match fields {
                    Ok(fields)=>{let mut changes=vec![];
                        egui::ScrollArea::vertical().id_salt("fields").max_height((ui.available_height()-100.0).max(80.0)).show(ui,|ui|{
                            egui::Grid::new(("config_parameters",self.id)).striped(true).num_columns(2).show(ui,|ui|{
                                for (key,mut value) in fields {
                                    let (status,help)=parameter_description(&key);
                                    ui.label(&key).on_hover_text(format!("{status}: {help}"));
                                    let changed=if value=="y"||value=="n" {let mut boolean=value=="y";let changed=ui.checkbox(&mut boolean,"").changed();value=if boolean{"y"}else{"n"}.into();changed}
                                    else{ui.add(egui::TextEdit::singleline(&mut value).id_salt((self.id,&key)).desired_width(175.0)).changed()};
                                    if changed{changes.push((key,value));}ui.end_row();
                                }
                            });
                        });
                        if !changes.is_empty(){let values:Vec<_>=changes.iter().map(|(k,v)|(k.as_str(),v.clone())).collect();act!(self,self.fields(&values));}
                    }
                    Err(e)=>{ui.colored_label(Color32::LIGHT_RED,e);}
                }
            }
            egui::CollapsingHeader::new("Compatibility / parameter execution").show(ui,|ui|{
                ui.label("Unknown fields and comments round-trip unchanged. Spherical mode uses q_factor, threshold controls, optimizer tolerance/evaluation budget and max error. Global search, sphere templates, serial/UDP and raw/debug recording use the shared Rust engine. Local/global optimizers differ from upstream NLopt; inspect quality and benchmark results.");
            });
        });
        egui::TopBottomPanel::bottom(egui::Id::new(("status", self.id))).show(ctx, |ui| {
            if !self.error.is_empty() {
                ui.colored_label(Color32::LIGHT_RED, &self.error);
            }
            ui.label(&self.snapshot.notice);
            ui.horizontal(|ui| {
                ui.label(format!(
                    "{} frames · {:.0} fit fps · {} capture frames skipped · config r{}",
                    self.snapshot.frames,
                    self.snapshot.processing_fps,
                    self.snapshot.dropped,
                    self.snapshot.revision
                ));
                if let Some(row) = &self.snapshot.row {
                    ui.label(format!(
                        "{} · error {:.4} · heading {:.3} rad · x {:.3}, y {:.3}",
                        row.status, row.error, row.heading, row.position[0], row.position[1]
                    ));
                }
            });
            ui.horizontal(|ui| {
                ui.label("Record to new folder:");
                ui.add(egui::TextEdit::singleline(&mut self.recording_path).desired_width(240.0));
                if self.snapshot.recording {
                    if ui.button("Stop recording").clicked() {
                        self.send(Command::StopRecord);
                    }
                } else if ui.button("Record data + configured videos").clicked() {
                    self.send(Command::Record(self.recording_path.clone().into()));
                }
                ui.label("No edits are written to disk until Save.");
            });
            if let Some(path) = &self.snapshot.recording_directory {
                ui.small(format!("Recording files: {}", path.display()));
            }
        });
        self.refresh_textures(ctx);
        egui::CentralPanel::default()
            .frame(
                egui::Frame::NONE
                    .fill(Color32::from_rgb(15, 18, 22))
                    .inner_margin(12.0),
            )
            .show(ctx, |ui| {
                let size = ui.available_size();
                let width = size.x.min(size.y * 4.0 / 3.0);
                let height = width * 0.75;
                let (rect, _) =
                    ui.allocate_exact_size(Vec2::new(width, height), egui::Sense::hover());
                self.canvas(ui, rect);
            });
        ctx.request_repaint_after(std::time::Duration::from_millis(16));
    }
    fn refresh_textures(&mut self, ctx: &egui::Context) {
        let key = self
            .snapshot
            .frames
            .wrapping_add(self.snapshot.revision.wrapping_shl(32));
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
        if let Some(d) = &frame.debug {
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
        let mut uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
        if let Some(image) = &self.snapshot.image
            && !self.full_image
            && let Ok(c) = self.core_config()
        {
            let f = c.height as f64 * 0.5 / (c.vfov_deg.to_radians() * 0.5).tan();
            let centre = DVec3::from_array(c.sphere.centre);
            let cx = f * centre.x / centre.z + c.width as f64 * 0.5;
            let cy = f * centre.y / centre.z + c.height as f64 * 0.5;
            let half = f * c.sphere.angular_radius.tan() * 1.5;
            uv = Rect::from_min_max(
                Pos2::new(
                    ((cx - half) / image.width as f64).clamp(0.0, 1.0) as f32,
                    ((cy - half) / image.height as f64).clamp(0.0, 1.0) as f32,
                ),
                Pos2::new(
                    ((cx + half) / image.width as f64).clamp(0.0, 1.0) as f32,
                    ((cy + half) / image.height as f64).clamp(0.0, 1.0) as f32,
                ),
            );
        }
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
                    target,
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
                (((p.x - rect.left()) / rect.width() * uv.width() + uv.left()) as f64 * w)
                    .clamp(0.0, w - 1.0),
                (((p.y - rect.top()) / rect.height() * uv.height() + uv.top()) as f64 * h)
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
        if let Ok(doc) = self.document() {
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
            let u = centre.cross(DVec3::Y).normalize();
            let v = centre.cross(u);
            let radius = config.sphere.angular_radius;
            let outline: Vec<_> = (0..=100)
                .map(|i| {
                    let a = i as f64 * std::f64::consts::TAU / 100.0;
                    let ray = centre * radius.cos() + (u * a.cos() + v * a.sin()) * radius.sin();
                    to_screen([f * ray.x / ray.z + w * 0.5, f * ray.y / ray.z + h * 0.5])
                })
                .collect();
            painter.add(egui::Shape::line(
                outline,
                Stroke::new(1.3_f32, Color32::LIGHT_GREEN),
            ));
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
            if response.dragged()
                && let Some(pos) = response.interact_pointer_pos()
                && let Some((poly, vertex)) = self.pending_drag
            {
                if poly == 0 {
                    if let Some(p) = rim.get_mut(vertex) {
                        *p = to_source(pos);
                    }
                    act!(self, self.commit_rim(&rim));
                } else if let Some(polygon) = polygons.get_mut(poly - 1) {
                    if let Some(p) = polygon.get_mut(vertex) {
                        *p = to_source(pos);
                    }
                    act!(self, self.commit_polygons(&polygons));
                }
            }
            if response.drag_stopped() {
                self.pending_drag = None;
            }
        } else if self.tool == Tool::Forward {
            if response.drag_started() {
                self.drag_start = response.interact_pointer_pos().map(to_source);
            }
            if let (Some(start), Some(pos)) = (self.drag_start, response.interact_pointer_pos()) {
                painter.arrow(
                    to_screen(start),
                    pos - to_screen(start),
                    Stroke::new(2.0_f32, YELLOW),
                );
            }
            if response.drag_stopped()
                && let (Some(start), Some(end)) =
                    (self.drag_start.take(), response.interact_pointer_pos())
            {
                let result = self
                    .core_config()
                    .and_then(|c| calibration::forward_rotation(&c, start, to_source(end)))
                    .and_then(|r| self.fields(&[("c2a_r", vector(&r))]));
                self.report(result);
            }
        } else if matches!(self.tool, Tool::Rim | Tool::Exclude | Tool::Square) {
            if response.clicked()
                && let Some(pos) = response.interact_pointer_pos()
            {
                self.points.push(to_source(pos));
                if self.tool == Tool::Square && self.points.len() == 4 {
                    act!(self, self.finish_points());
                }
            }
            if response.double_clicked()
                || ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter))
            {
                act!(self, self.finish_points());
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
    fn remove_exclusion(&mut self) -> Result<(), String> {
        let doc = self.document()?;
        let values: Vec<Vec<f64>> = doc.vector("roi_ignr").unwrap_or_default();
        let mut polygons: Vec<_> = values
            .into_iter()
            .map(|v| v.as_chunks::<2>().0.to_vec())
            .collect();
        polygons.pop();
        self.commit_polygons(&polygons)
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
                let image = self.snapshot.image.as_ref().ok_or("No image loaded")?;
                let doc = self.document()?;
                let (r, t) = calibration::lab_from_square(
                    image.width,
                    image.height,
                    doc.number("vfov", 45.0),
                    doc.boolean("fisheye"),
                    &self.plane,
                    &points,
                )?;
                let key = format!("c2a_cnrs_{}", self.plane);
                self.fields(&[
                    (
                        key.as_str(),
                        vector(&points.iter().flatten().copied().collect::<Vec<_>>()),
                    ),
                    ("c2a_src", key.clone()),
                    ("c2a_r", vector(&r)),
                    ("c2a_t", vector(&t)),
                ])?;
            }
            _ => return Ok(()),
        }
        self.points.clear();
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
}
