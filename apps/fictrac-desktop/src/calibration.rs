//! Desktop adapters for the shared headless calibration engine.
use crate::{
    runtime::Command,
    session::{Session, Tool},
};
use eframe::egui::{Pos2, Rect, Vec2};
use fictrac_core::{Config, Sphere};
use fictrac_io::ConfigDocument;
use glam::DVec3;

fn legacy<T: serde::Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string(value)
        .map(|s| s.replace('[', "{").replace(']', "}"))
        .map_err(|e| e.to_string())
}

/// Reserve conservative native border/title space as well as the content.
/// Prefer right, then left; clamp an overlapping inspector only when neither
/// side fits. Small displays shrink the content instead of putting it offscreen.
pub(crate) fn inspector_layout(window: Rect, monitor: Vec2) -> Option<(Rect, Vec2)> {
    let chrome = Vec2::new(16.0, 40.0);
    if ![
        window.left(),
        window.top(),
        window.right(),
        window.bottom(),
        monitor.x,
        monitor.y,
    ]
    .iter()
    .all(|v| v.is_finite())
        || monitor.x <= chrome.x
        || monitor.y <= chrome.y
    {
        return None;
    }
    let size = Vec2::new(456.0_f32.min(monitor.x), 680.0_f32.min(monitor.y));
    let right = window.right() + 8.0;
    let left = window.left() - size.x - 8.0;
    let x = if right >= 0.0 && right + size.x <= monitor.x {
        right
    } else if left >= 0.0 && left + size.x <= monitor.x {
        left
    } else {
        right.clamp(0.0, monitor.x - size.x)
    };
    let y = window.top().clamp(0.0, monitor.y - size.y);
    Some((Rect::from_min_size(Pos2::new(x, y), size), size - chrome))
}

/// Prefer a separate loupe beside/below the preview, never over its targets or
/// the configuration inspector reserved by the same layout calculation.
pub(crate) fn magnifier_position(
    window: Rect,
    monitor: Vec2,
    size: Vec2,
    inspector: Option<Rect>,
) -> Option<Pos2> {
    let screen = Rect::from_min_size(Pos2::ZERO, monitor);
    if ![
        window.left(),
        window.top(),
        window.right(),
        window.bottom(),
        monitor.x,
        monitor.y,
        size.x,
        size.y,
    ]
    .iter()
    .all(|v| v.is_finite())
        || size.x <= 0.0
        || size.y <= 0.0
        || size.x > monitor.x
        || size.y > monitor.y
    {
        return None;
    }
    for position in [
        Pos2::new(window.left() - size.x - 8.0, window.top()),
        Pos2::new(window.left(), window.bottom() + 8.0),
        Pos2::new(window.right() + 8.0, window.top()),
        Pos2::new(window.left(), window.top() - size.y - 8.0),
    ] {
        let position = Pos2::new(
            position.x.clamp(0.0, monitor.x - size.x),
            position.y.clamp(0.0, monitor.y - size.y),
        );
        let rect = Rect::from_min_size(position, size);
        if screen.contains_rect(rect)
            && !window.intersects(rect)
            && inspector.is_none_or(|reserved| !reserved.intersects(rect))
        {
            return Some(position);
        }
    }
    None
}
pub(crate) fn magnifier_inset(
    bounds: Rect,
    cursor: Pos2,
    size: Vec2,
    inspector: Option<Rect>,
) -> Option<Rect> {
    if bounds.width() < size.x || bounds.height() < size.y {
        return None;
    }
    [
        bounds.left_top(),
        Pos2::new(bounds.right() - size.x, bounds.top()),
        Pos2::new(bounds.left(), bounds.bottom() - size.y),
        bounds.right_bottom() - size,
    ]
    .into_iter()
    .map(|p| Rect::from_min_size(p, size))
    .filter_map(|r| {
        let distance = r.clamp(cursor).distance(cursor);
        (distance >= 24.0 && inspector.is_none_or(|reserved| !reserved.intersects(r)))
            .then_some((distance, r))
    })
    .max_by(|a, b| a.0.total_cmp(&b.0))
    .map(|(_, r)| r)
}

impl Session {
    pub(crate) fn validated_calibration_document(
        &self,
        document: ConfigDocument,
    ) -> Result<ConfigDocument, String> {
        let image = self
            .snapshot
            .image
            .as_ref()
            .ok_or("Wait for a source image before placing calibration marks")?;
        fictrac_io::load_config_text(document.text(), image.width, image.height)
            .map_err(|e| e.to_string())?;
        Ok(document)
    }

    /// Derive the in-progress view through the same document operation used on
    /// Continue. Incomplete marks never replace the saved configuration.
    pub(crate) fn calibration_preview_config(&self) -> Result<Option<Config>, String> {
        if self.calibration_review
            || !matches!(self.tool, Tool::Rim | Tool::Square)
            || self.points.len() < if self.tool == Tool::Rim { 3 } else { 4 }
        {
            return Ok(None);
        }
        let Some(document) = self.calibration_draft_document()? else {
            return Ok(None);
        };
        let image = self
            .snapshot
            .image
            .as_ref()
            .ok_or("Wait for a source image before placing calibration marks")?;
        fictrac_io::load_config_text(document.text(), image.width, image.height)
            .map(|loaded| Some(loaded.config))
            .map_err(|e| e.to_string())
    }

    pub(crate) fn calibration_progress(&self) -> Result<String, String> {
        if self.calibration_review {
            return Ok("Review the saved marks, or choose Redraw in Settings.".into());
        }
        if self.edit_existing {
            self.validated_calibration_document(self.document()?)?;
            return Ok("Drag to adjust · right-click to remove · continue when ready".into());
        }
        let count = self.points.len();
        match self.tool {
            Tool::Rim if count < 3 => Ok(format!(
                "{count} of 3 rim marks · spread them around the visible ball edge"
            )),
            Tool::Rim => self
                .calibration_preview_config()
                .map(|_| format!("Live fit · {count} rim marks · add more to refine the outline")),
            Tool::Square if count < 4 => Ok(format!(
                "Next corner: {} · clockwise around the square",
                ["top left", "top right", "bottom right", "bottom left"][count]
            )),
            Tool::Square => self
                .calibration_preview_config()
                .map(|_| "Live orientation · check the coloured lab axes".into()),
            Tool::Exclude if count < 3 => Ok(format!(
                "{count} of 3 region marks · outline a holder, animal or reflection to ignore"
            )),
            Tool::Exclude => self
                .calibration_draft_document()
                .map(|_| format!("{count}-point region · add marks to refine its boundary")),
            _ => Ok(String::new()),
        }
    }

    pub(crate) fn cancel_calibration(&mut self) {
        self.points.clear();
        self.square_slots = None;
        self.pending_drag = None;
        self.calibration_review = true;
        self.calibration_redraw = false;
        self.edit_existing = false;
        self.tool = Tool::Inspect;
        self.full_image = false;
        self.show_overlays = false;
        self.paused = self
            .calibration_previous_paused
            .take()
            .unwrap_or(self.paused);
        self.send(Command::Pause(self.paused));
    }

    pub(crate) fn calibration_lines(&self) -> Vec<Vec<[f64; 2]>> {
        let preview = self.calibration_preview_config().ok().flatten();
        if self.calibration_redraw && preview.is_none() && self.tool != Tool::Exclude {
            return vec![];
        }
        let close = |mut points: Vec<[f64; 2]>| {
            if let Some(first) = points.first().copied() {
                points.push(first);
            }
            points
        };
        match self.tool {
            Tool::Rim => {
                let Ok(config) = preview.map(Ok).unwrap_or_else(|| self.core_config()) else {
                    return vec![];
                };
                let Ok(geometry) = fictrac_core::Geometry::new(config.clone()) else {
                    return vec![];
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
                vec![
                    (0..=100)
                        .filter_map(|i| {
                            let a = i as f64 * std::f64::consts::TAU / 100.0;
                            geometry.project_ray(
                                centre * radius.cos() + (u * a.cos() + v * a.sin()) * radius.sin(),
                            )
                        })
                        .collect(),
                ]
            }
            Tool::Exclude if !self.points.is_empty() => vec![close(self.points.clone())],
            Tool::Exclude if self.calibration_redraw => vec![],
            Tool::Exclude => self
                .document()
                .ok()
                .and_then(|d| d.vector::<Vec<Vec<f64>>>("roi_ignr").ok())
                .unwrap_or_default()
                .into_iter()
                .map(|v| close(v.as_chunks::<2>().0.to_vec()))
                .collect(),
            Tool::Square if preview.is_some() => vec![close(self.points.clone())],
            Tool::Square => self
                .document()
                .ok()
                .and_then(|d| {
                    d.vector::<Vec<f64>>(d.get("c2a_src").unwrap_or("c2a_cnrs_xy"))
                        .ok()
                })
                .map(|v| vec![close(v.as_chunks::<2>().0.to_vec())])
                .unwrap_or_default(),
            _ => vec![],
        }
    }
    pub(crate) fn begin_calibration_step(&mut self) {
        self.points.clear();
        self.square_slots = None;
        self.pending_drag = None;
        self.edit_existing = false;
        self.calibration_review = true;
        self.calibration_redraw = false;
        if self.tool != Tool::Inspect {
            self.calibration_previous_paused.get_or_insert(self.paused);
            self.full_image = true;
            self.show_diagnostics = false;
            self.show_overlays = false;
            self.paused = true;
            self.send(Command::Pause(true));
            if self.tool == Tool::Square
                && let Ok(doc) = self.document()
                && let Some(plane) = doc
                    .get("c2a_src")
                    .and_then(|key| key.strip_prefix("c2a_cnrs_"))
                && matches!(plane, "xy" | "yz" | "xz")
            {
                self.plane = plane.into();
            }
        }
    }
    /// The primary graphical action is replacement, ready for the first click.
    /// Saved-mark review/edit stays available as a secondary action.
    pub(crate) fn redraw_calibration(&mut self, tool: Tool) {
        self.tool = tool;
        self.begin_calibration_step();
        if matches!(tool, Tool::Rim | Tool::Exclude | Tool::Square) {
            self.calibration_review = false;
            self.calibration_redraw = true;
        }
    }
    pub(crate) fn saved_mark_count(&self) -> usize {
        self.saved_mark_count_for(self.tool)
    }
    fn saved_mark_count_for(&self, tool: Tool) -> usize {
        let Ok(doc) = self.document() else {
            return 0;
        };
        match tool {
            Tool::Rim => doc
                .vector::<Vec<f64>>("roi_circ")
                .map(|v| v.len() / 2)
                .unwrap_or(0),
            Tool::Exclude => doc
                .vector::<Vec<Vec<f64>>>("roi_ignr")
                .map(|v| v.iter().map(|p| p.len() / 2).sum())
                .unwrap_or(0),
            Tool::Square => doc
                .vector::<Vec<f64>>(doc.get("c2a_src").unwrap_or("c2a_cnrs_xy"))
                .map(|v| v.len() / 2)
                .unwrap_or(0),
            _ => 0,
        }
    }
    pub(crate) fn saved_calibration_summary(&self) -> String {
        let count = self.saved_mark_count();
        match self.tool {
            Tool::Rim if count == 0 => "Saved angular sphere geometry; no editable rim clicks were stored. Keep it or redraw new rim points.".into(),
            Tool::Rim => format!("Saved ball outline · {count} rim points"),
            Tool::Exclude => {
                let polygons = self.document().ok().and_then(|doc| doc.vector::<Vec<Vec<f64>>>("roi_ignr").ok()).map(|v| v.len()).unwrap_or(0);
                format!("{polygons} saved exclusion regions · {count} vertices (optional)")
            }
            Tool::Square if count == 0 => "Saved lab rotation; no editable square corners were stored. Keep it or redraw a square.".into(),
            Tool::Square => format!("Saved {} lab square · {count} ordered corners", self.plane.to_uppercase()),
            _ => "Current saved calibration".into(),
        }
    }
    pub(crate) fn choose_calibration_action(&mut self, action: &str) -> Result<(), String> {
        self.validate_calibration_action(self.tool, action)?;
        if action == "keep" {
            // Keep always means the saved layer, including an empty redraw.
            // A draft must use Continue/Add this region to replace that layer.
            self.points.clear();
            self.square_slots = None;
            self.pending_drag = None;
            self.calibration_review = true;
            self.calibration_redraw = false;
            self.edit_existing = false;
            return self.advance_calibration_step();
        }
        if self.tool == Tool::Inspect {
            return Err("Choose Ball, Exclusions or Lab before editing calibration".into());
        }
        match action {
            "inspect" | "redraw" => {}
            "edit" if self.saved_mark_count() > 0 => {}
            "edit" => {
                return Err(
                    "No saved click points exist for this step; choose Redraw instead".into(),
                );
            }
            "add" if self.tool == Tool::Exclude => {}
            "add" => return Err("Adding another region is available only for exclusions".into()),
            _ => return Err("calibration_action must be inspect/edit/redraw/add/keep".into()),
        }
        self.points.clear();
        self.square_slots = None;
        self.pending_drag = None;
        self.calibration_review = action == "inspect";
        self.edit_existing = action == "edit";
        self.calibration_redraw = action == "redraw";
        Ok(())
    }
    pub(crate) fn validate_calibration_action(
        &self,
        tool: Tool,
        action: &str,
    ) -> Result<(), String> {
        if action == "keep" {
            return Ok(());
        }
        if tool == Tool::Inspect {
            return Err("Choose Ball, Exclusions or Lab before editing calibration".into());
        }
        match action {
            "inspect" | "redraw" => Ok(()),
            "edit" if self.saved_mark_count_for(tool) > 0 => Ok(()),
            "edit" => {
                Err("No saved click points exist for this step; choose Redraw instead".into())
            }
            "add" if tool == Tool::Exclude => Ok(()),
            "add" => Err("Adding another region is available only for exclusions".into()),
            _ => Err("calibration_action must be inspect/edit/redraw/add/keep".into()),
        }
    }
    pub(crate) fn calibration_document(
        &self,
        tool: Tool,
        points: &[[f64; 2]],
        plane: &str,
        replace: bool,
    ) -> Result<ConfigDocument, String> {
        let mut doc = self.document()?;
        let image = self
            .snapshot
            .image
            .as_ref()
            .ok_or("No source image is available for calibration")?;
        match tool {
            Tool::Rim => {
                let mut config = self.core_config().unwrap_or_else(|_| Config::default());
                config.width = image.width;
                config.height = image.height;
                config.vfov_deg = doc.number("vfov", 45.0);
                config.fisheye = doc
                    .get("fisheye")
                    .is_some_and(|v| matches!(v, "y" | "Y" | "true" | "1"));
                let sphere = fit_rim(&config, points)?;
                doc.set(
                    "roi_circ",
                    &legacy(&points.iter().flatten().copied().collect::<Vec<_>>())?,
                )
                .map_err(|e| e.to_string())?;
                doc.set("roi_c", &legacy(&sphere.centre)?)
                    .map_err(|e| e.to_string())?;
                doc.set("roi_r", &sphere.angular_radius.to_string())
                    .map_err(|e| e.to_string())?;
            }
            Tool::Square => {
                let (r, t) = lab_from_square(
                    image.width,
                    image.height,
                    doc.number("vfov", 45.0),
                    doc.get("fisheye")
                        .is_some_and(|v| matches!(v, "y" | "Y" | "true" | "1")),
                    plane,
                    points,
                )?;
                let key = format!("c2a_cnrs_{plane}");
                for (key, value) in [
                    (
                        key.as_str(),
                        legacy(&points.iter().flatten().copied().collect::<Vec<_>>())?,
                    ),
                    ("c2a_src", key.clone()),
                    ("c2a_r", legacy(&r)?),
                    ("c2a_t", legacy(&t)?),
                ] {
                    doc.set(key, &value).map_err(|e| e.to_string())?;
                }
            }
            Tool::Exclude => {
                let old: Vec<Vec<f64>> = if replace {
                    vec![]
                } else {
                    doc.vector("roi_ignr").unwrap_or_default()
                };
                let mut polygons: Vec<_> = old
                    .into_iter()
                    .map(|p| p.as_chunks::<2>().0.to_vec())
                    .collect();
                if !points.is_empty() {
                    polygons.push(points.to_vec());
                }
                let document = fictrac_control::replace_exclusions_document(&doc, &polygons)?;
                return self.validated_calibration_document(document);
            }
            _ => return Err("Select Ball, Exclusions or Lab before applying calibration".into()),
        }
        if doc.get("rust_calibration_size").is_some() {
            let [width, height]: [usize; 2] = doc
                .vector("rust_calibration_size")
                .map_err(|e| e.to_string())?;
            doc = fictrac_io::capture_calibration_reference(&doc, width, height)
                .map_err(|e| e.to_string())?;
        }
        self.validated_calibration_document(doc)
    }
    pub(crate) fn calibration_draft_document(&self) -> Result<Option<ConfigDocument>, String> {
        let mut points = self.points.clone();
        points.dedup();
        if points.len() > 1 && points.first() == points.last() {
            points.pop();
        }
        if points.is_empty() {
            if !self.calibration_redraw {
                return Ok(None);
            }
            if self.tool != Tool::Exclude {
                return Err("The replacement draft is empty. Place valid points or cancel the draft to keep the saved calibration".into());
            }
        }
        self.calibration_document(self.tool, &points, &self.plane, self.calibration_redraw)
            .map(Some)
    }
    fn apply_calibration_draft(&mut self) -> Result<(), String> {
        if let Some(document) = self.calibration_draft_document()? {
            self.commit_calibration_document(document)?;
        } else if self.edit_existing {
            self.validated_calibration_document(self.document()?)?;
        }
        Ok(())
    }
    pub(crate) fn commit_calibration_document(
        &mut self,
        document: ConfigDocument,
    ) -> Result<(), String> {
        let history = self.history.clone();
        let changed = self.changed;
        let revision = self.revision;
        self.edit(document.text().to_owned());
        if let Err(error) = self.apply() {
            self.history = history;
            self.changed = changed;
            self.revision = revision;
            return Err(error);
        }
        self.points.clear();
        self.square_slots = None;
        self.calibration_review = true;
        self.calibration_redraw = false;
        self.edit_existing = false;
        Ok(())
    }
    pub(crate) fn advance_calibration_step(&mut self) -> Result<(), String> {
        self.apply_calibration_draft()?;
        self.tool = match self.tool {
            Tool::Rim => Tool::Exclude,
            Tool::Exclude => Tool::Square,
            _ => Tool::Inspect,
        };
        if self.tool == Tool::Inspect {
            return self.complete_calibration();
        }
        self.redraw_calibration(self.tool);
        Ok(())
    }
    pub(crate) fn complete_calibration(&mut self) -> Result<(), String> {
        self.apply_calibration_draft()?;
        self.tool = Tool::Inspect;
        self.points.clear();
        self.square_slots = None;
        self.calibration_review = true;
        self.calibration_redraw = false;
        self.edit_existing = false;
        self.calibration_previous_paused = None;
        self.show_overlays = false;
        self.full_image = false;
        self.paused = false;
        self.send(Command::Pause(false));
        Ok(())
    }
}
pub fn lab_from_square(
    width: usize,
    height: usize,
    vfov: f64,
    fisheye: bool,
    plane: &str,
    points: &[[f64; 2]],
) -> Result<([f64; 3], [f64; 3]), String> {
    fictrac_core::calibration::lab_from_square(width, height, vfov, fisheye, plane, points)
        .map_err(|e| e.to_string())
}
pub fn fit_rim(config: &Config, points: &[[f64; 2]]) -> Result<Sphere, String> {
    fictrac_core::calibration::fit_rim(config, points).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn precision_lens_prefers_space_outside_preview_and_never_covers_pointer() {
        let window = Rect::from_min_size(Pos2::new(200.0, 100.0), Vec2::new(500.0, 400.0));
        let monitor = Vec2::new(1600.0, 1000.0);
        let size = Vec2::new(240.0, 264.0);
        let position = magnifier_position(window, monitor, size, None);
        assert!(position.is_some());
        if let Some(position) = position {
            assert!(!Rect::from_min_size(position, size).intersects(window));
        }
        assert!(
            magnifier_position(
                Rect::from_min_size(Pos2::ZERO, monitor),
                monitor,
                size,
                None
            )
            .is_none()
        );
        let cursor = Pos2::new(250.0, 150.0);
        let inset = magnifier_inset(window, cursor, size, None);
        assert!(inset.is_some());
        if let Some(inset) = inset {
            assert!(!inset.expand(24.0).contains(cursor));
            assert!(window.contains_rect(inset));
        }
    }
    #[test]
    fn inspector_chooses_right_then_left_and_stays_on_screen() -> Result<(), String> {
        for (window, monitor, right_expected) in [
            (
                Rect::from_min_size(Pos2::new(20.0, 40.0), Vec2::new(500.0, 400.0)),
                Vec2::new(1600.0, 1000.0),
                Some(true),
            ),
            (
                Rect::from_min_size(Pos2::new(1080.0, 600.0), Vec2::new(500.0, 400.0)),
                Vec2::new(1600.0, 1000.0),
                Some(false),
            ),
            (
                Rect::from_min_size(Pos2::ZERO, Vec2::new(640.0, 480.0)),
                Vec2::new(640.0, 480.0),
                None,
            ),
            (
                Rect::from_min_size(Pos2::ZERO, Vec2::new(320.0, 240.0)),
                Vec2::new(320.0, 240.0),
                None,
            ),
        ] {
            let (reserved, inner) =
                inspector_layout(window, monitor).ok_or("valid display had no layout")?;
            assert!(Rect::from_min_size(Pos2::ZERO, monitor).contains_rect(reserved));
            assert!(inner.x > 0.0 && inner.y > 0.0);
            if let Some(right) = right_expected {
                if right {
                    assert!(reserved.left() > window.right());
                } else {
                    assert!(reserved.right() < window.left());
                }
            }
        }
        Ok(())
    }
    #[test]
    fn precision_lens_avoids_right_and_left_inspector_reservations() -> Result<(), String> {
        let monitor = Vec2::new(1600.0, 1000.0);
        let size = Vec2::new(240.0, 264.0);
        for window in [
            Rect::from_min_size(Pos2::new(20.0, 40.0), Vec2::new(500.0, 400.0)),
            Rect::from_min_size(Pos2::new(1080.0, 40.0), Vec2::new(500.0, 400.0)),
        ] {
            let (inspector, _) = inspector_layout(window, monitor).ok_or("missing inspector")?;
            let position = magnifier_position(window, monitor, size, Some(inspector));
            assert!(position.is_some());
            if let Some(position) = position {
                let lens = Rect::from_min_size(position, size);
                assert!(!lens.intersects(inspector));
                assert!(!lens.intersects(window));
            }
        }
        let full = Rect::from_min_size(Pos2::ZERO, monitor);
        assert!(
            magnifier_position(
                full,
                monitor,
                size,
                inspector_layout(full, monitor).map(|(r, _)| r)
            )
            .is_none()
        );
        let reserved = Rect::from_min_size(Pos2::new(1000.0, 0.0), Vec2::new(600.0, 1000.0));
        let inset = magnifier_inset(full, Pos2::new(100.0, 100.0), size, Some(reserved));
        assert!(inset.is_some());
        if let Some(inset) = inset {
            assert!(!inset.intersects(reserved));
        }
        Ok(())
    }
}
