//! Small native vector icons. Drawn in Rust; no font glyph or image dependency.
use eframe::egui::{self, Color32, Pos2, Stroke, Vec2};
#[derive(Clone, Copy)]
pub enum Icon {
    Reset,
    Record,
    Settings,
    Diagnostics,
    Refresh,
}
pub fn button(ui: &mut egui::Ui, icon: Icon, active: bool, help: &str) -> egui::Response {
    let response = ui.add(
        egui::Button::new("")
            .min_size(Vec2::splat(28.0))
            .selected(active)
            .frame(active),
    );
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), help));
    let color = if matches!(icon, Icon::Record) && active {
        Color32::from_rgb(245, 90, 100)
    } else {
        ui.visuals().text_color()
    };
    let stroke = Stroke::new(1.6_f32, color);
    let rect = response.rect.shrink(5.0);
    let p = |x: f32, y: f32| {
        Pos2::new(
            rect.left() + x / 24.0 * rect.width(),
            rect.top() + y / 24.0 * rect.height(),
        )
    };
    let line = |points: &[[f32; 2]]| {
        ui.painter().add(egui::Shape::line(
            points.iter().map(|v| p(v[0], v[1])).collect(),
            stroke,
        ));
    };
    match icon {
        Icon::Reset | Icon::Refresh => {
            let points: Vec<_> = (0..=30)
                .map(|i| {
                    let a = (-0.7 + i as f32 / 30.0 * 5.2) * std::f32::consts::PI / 3.0;
                    p(12.0 + 8.0 * a.cos(), 12.0 + 8.0 * a.sin())
                })
                .collect();
            ui.painter().add(egui::Shape::line(points, stroke));
            line(&[[15.0, 2.0], [21.0, 5.0], [20.0, 11.0]]);
        }
        Icon::Record => {
            if active {
                ui.painter().rect_filled(
                    egui::Rect::from_min_max(p(5.0, 5.0), p(19.0, 19.0)),
                    2.0,
                    color,
                );
            } else {
                ui.painter()
                    .circle_stroke(p(12.0, 12.0), rect.width() * 0.38, stroke);
                ui.painter()
                    .circle_filled(p(12.0, 12.0), rect.width() * 0.23, color);
            }
        }
        Icon::Settings => {
            for (y, x) in [(5.0, 8.0), (12.0, 16.0), (19.0, 10.0)] {
                line(&[[2.0, y], [22.0, y]]);
                ui.painter()
                    .circle_filled(p(x, y), 3.0, ui.visuals().panel_fill);
                ui.painter().circle_stroke(p(x, y), 3.0, stroke);
            }
        }
        Icon::Diagnostics => {
            for (x, y) in [(2.0, 3.0), (14.0, 3.0), (2.0, 14.0), (14.0, 14.0)] {
                line(&[
                    [x, y],
                    [x + 8.0, y],
                    [x + 8.0, y + 7.0],
                    [x, y + 7.0],
                    [x, y],
                ]);
            }
        }
    }
    response.on_hover_text(help)
}
