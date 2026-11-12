//! Estilo sobrio: fondo neutro, un color de acento y controles cómodos al tacto.

use super::widgets::ACCENT;
use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, TextStyle, Theme, Visuals};

pub fn install(ctx: &egui::Context) {
    for theme in [Theme::Light, Theme::Dark] {
        let mut visuals = match theme {
            Theme::Light => Visuals::light(),
            Theme::Dark => Visuals::dark(),
        };
        visuals.selection.bg_fill = ACCENT.linear_multiply(0.55);
        visuals.selection.stroke.color = Color32::WHITE;
        visuals.hyperlink_color = ACCENT;
        let radius = CornerRadius::same(6);
        visuals.widgets.inactive.corner_radius = radius;
        visuals.widgets.hovered.corner_radius = radius;
        visuals.widgets.active.corner_radius = radius;
        visuals.widgets.noninteractive.corner_radius = radius;
        if theme == Theme::Light {
            visuals.panel_fill = Color32::from_rgb(0xf6, 0xf7, 0xf7);
            visuals.faint_bg_color = Color32::WHITE;
        } else {
            visuals.panel_fill = Color32::from_rgb(0x17, 0x1a, 0x1b);
            visuals.faint_bg_color = Color32::from_rgb(0x20, 0x24, 0x25);
        }
        ctx.set_visuals_of(theme, visuals);
    }
    ctx.all_styles_mut(|style| {
        style.spacing.button_padding = egui::vec2(10.0, 6.0);
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.interact_size.y = 30.0;
        style
            .text_styles
            .insert(TextStyle::Body, FontId::new(15.0, FontFamily::Proportional));
        style.text_styles.insert(
            TextStyle::Button,
            FontId::new(15.0, FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Heading,
            FontId::new(22.0, FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Small,
            FontId::new(12.5, FontFamily::Proportional),
        );
    });
}
