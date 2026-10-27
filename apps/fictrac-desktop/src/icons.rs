//! Original Lucide artwork, rendered offline and tinted by the native UI.
//! Pinned SVG sources and ISC/Feather MIT notices live in assets/lucide.
use eframe::egui::{self, TextureHandle, Vec2};
#[derive(Clone, Copy)]
pub enum Icon {
    Reset,
    Record,
    Settings,
    Diagnostics,
    Refresh,
    Crop,
    More,
    Camera,
    Calibration,
    Play,
    Pause,
    Step,
}
fn artwork(icon: Icon, active: bool) -> (&'static str, &'static [u8]) {
    macro_rules! asset {
        ($name:literal) => {
            (
                $name,
                include_bytes!(concat!("../../../assets/lucide/", $name, ".png")),
            )
        };
    }
    match icon {
        Icon::Reset => asset!("rotate-ccw"),
        Icon::Record if active => asset!("square"),
        Icon::Record => asset!("circle-dot"),
        Icon::Settings => asset!("sliders-horizontal"),
        Icon::Diagnostics => asset!("layout-dashboard"),
        Icon::Refresh => asset!("refresh-cw"),
        Icon::Crop if active => asset!("maximize"),
        Icon::Crop => asset!("minimize"),
        Icon::More => asset!("ellipsis"),
        Icon::Camera => asset!("camera"),
        Icon::Calibration => asset!("circle-dot"),
        Icon::Play => asset!("play"),
        Icon::Pause => asset!("pause"),
        Icon::Step => asset!("skip-forward"),
    }
}
fn native_button(ui: &egui::Ui, icon: Icon, active: bool) -> egui::Button<'static> {
    if matches!(icon, Icon::Calibration) {
        return egui::Button::new("")
            .min_size(Vec2::splat(30.0))
            .selected(active)
            .frame(active);
    }
    let (name, bytes) = artwork(icon, active);
    let key = egui::Id::new(("lucide", name));
    let texture = ui
        .ctx()
        .data_mut(|data| data.get_temp::<TextureHandle>(key));
    let texture = texture.or_else(|| {
        let decoded = match image::load_from_memory(bytes) {
            Ok(image) => image.into_rgba8(),
            Err(error) => {
                eprintln!("Cannot read bundled Lucide icon {name}: {error}");
                return None;
            }
        };
        let pixels = egui::ColorImage::from_rgba_unmultiplied(
            [decoded.width() as usize, decoded.height() as usize],
            decoded.as_raw(),
        );
        let handle = ui
            .ctx()
            .load_texture(name, pixels, egui::TextureOptions::LINEAR);
        ui.ctx()
            .data_mut(|data| data.insert_temp(key, handle.clone()));
        Some(handle)
    });
    let color = if matches!(icon, Icon::Record) {
        ui.visuals().error_fg_color
    } else {
        ui.visuals().text_color()
    };
    let button = if let Some(texture) = texture {
        egui::Button::image(egui::Image::new((texture.id(), Vec2::splat(18.0))).tint(color))
    } else {
        egui::Button::new("Icon unavailable")
    };
    let button = if matches!(icon, Icon::Record) && active {
        button.fill(color.gamma_multiply(0.18))
    } else {
        button
    };
    button
        .min_size(Vec2::splat(30.0))
        .selected(active)
        .frame(active)
}
pub fn button(ui: &mut egui::Ui, icon: Icon, active: bool, help: &str) -> egui::Response {
    let response = ui.add(native_button(ui, icon, active));
    if matches!(icon, Icon::Calibration) {
        let center = response.rect.center();
        let stroke = egui::Stroke::new(1.6_f32, ui.visuals().text_color());
        ui.painter().circle_stroke(center, 6.5, stroke);
        for direction in [
            egui::vec2(1.0, 0.0),
            egui::vec2(-1.0, 0.0),
            egui::vec2(0.0, 1.0),
            egui::vec2(0.0, -1.0),
        ] {
            ui.painter().line_segment(
                [center + direction * 4.0, center + direction * 10.0],
                stroke,
            );
        }
    }
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), help));
    response.on_hover_text(help)
}
pub fn menu<R>(
    ui: &mut egui::Ui,
    icon: Icon,
    help: &str,
    content: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<Option<R>> {
    let (response, inner) =
        egui::containers::menu::MenuButton::from_button(native_button(ui, icon, false))
            .ui(ui, content);
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), help));
    egui::InnerResponse {
        response: response.on_hover_text(help),
        inner: inner.map(|r| r.inner),
    }
}
