//! Gráficos dibujados con el pintor de egui: barras finas con extremo
//! redondeado, líneas de 2 px, rejilla tenue, leyenda y ayuda al pasar el cursor.

use caudal_app_state::charts::{Chart, SERIES_COLORS};
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, Ui, vec2,
};

fn hex(value: &str) -> Color32 {
    let value = value.trim_start_matches('#');
    let channel =
        |range: std::ops::Range<usize>| u8::from_str_radix(&value[range], 16).unwrap_or(0);
    Color32::from_rgb(channel(0..2), channel(2..4), channel(4..6))
}

pub fn series_color(ui: &Ui, slot: usize) -> Color32 {
    let (light, dark) = SERIES_COLORS[slot.min(SERIES_COLORS.len() - 1)];
    hex(if ui.visuals().dark_mode { dark } else { light })
}

/// Leyenda: muestra de color junto al nombre, en color de texto.
pub fn legend(ui: &mut Ui, chart: &Chart) {
    if chart.series.len() < 2 {
        return;
    }
    ui.horizontal(|ui| {
        for series in &chart.series {
            let (rect, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
            ui.painter()
                .rect_filled(rect, CornerRadius::same(2), series_color(ui, series.slot));
            ui.label(egui::RichText::new(&series.name).small());
            ui.add_space(8.0);
        }
    });
}

struct Plot {
    rect: Rect,
    plot: Rect,
}

fn frame(ui: &mut Ui, chart: &Chart, height: f32) -> (Plot, egui::Response) {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let label_width = 58.0;
    let plot = Rect::from_min_max(
        rect.min + vec2(label_width, 6.0),
        rect.max - vec2(4.0, 22.0),
    );
    let painter = ui.painter_at(rect);
    let grid = ui.visuals().widgets.noninteractive.bg_stroke.color;
    let muted = ui.visuals().weak_text_color();
    for tick in &chart.ticks {
        let y = plot.bottom() - chart.fraction(tick.value) as f32 * plot.height();
        painter.line_segment(
            [Pos2::new(plot.left(), y), Pos2::new(plot.right(), y)],
            Stroke::new(1.0, grid),
        );
        painter.text(
            Pos2::new(plot.left() - 6.0, y),
            Align2::RIGHT_CENTER,
            &tick.label,
            FontId::proportional(11.0),
            muted,
        );
    }
    // Etiquetas del eje x: todas si caben, si no, una de cada dos.
    let count = chart.labels.len().max(1);
    let band = plot.width() / count as f32;
    let every = if band < 30.0 { 2 } else { 1 };
    for (index, label) in chart.labels.iter().enumerate() {
        if !(count - 1 - index).is_multiple_of(every) {
            continue;
        }
        let x = plot.left() + band * (index as f32 + 0.5);
        painter.text(
            Pos2::new(x, plot.bottom() + 4.0),
            Align2::CENTER_TOP,
            label,
            FontId::proportional(11.0),
            muted,
        );
    }
    (Plot { rect, plot }, response)
}

fn hovered_index(response: &egui::Response, plot: Rect, count: usize) -> Option<usize> {
    let pointer = response.hover_pos()?;
    if !plot.expand2(vec2(0.0, 20.0)).contains(pointer) || count == 0 {
        return None;
    }
    let band = plot.width() / count as f32;
    Some((((pointer.x - plot.left()) / band).floor() as usize).min(count - 1))
}

fn tooltip(ui: &Ui, response: &egui::Response, chart: &Chart, index: usize) {
    let label = chart.labels.get(index).cloned().unwrap_or_default();
    response.clone().on_hover_ui_at_pointer(|ui| {
        ui.label(egui::RichText::new(label).strong());
        for series in &chart.series {
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
                ui.painter().rect_filled(
                    rect,
                    CornerRadius::same(2),
                    series_color(ui, series.slot),
                );
                ui.label(format!(
                    "{}: {}",
                    series.name,
                    series.texts.get(index).cloned().unwrap_or_default()
                ));
            });
        }
    });
    let _ = ui;
}

/// Columnas agrupadas que crecen desde la línea base.
pub fn bar_chart(ui: &mut Ui, chart: &Chart, height: f32) {
    legend(ui, chart);
    let (Plot { rect, plot }, response) = frame(ui, chart, height);
    let painter = ui.painter_at(rect);
    let count = chart.labels.len().max(1);
    let band = plot.width() / count as f32;
    let groups = chart.series.len().max(1) as f32;
    let bar = ((band * 0.7 - 2.0 * (groups - 1.0)) / groups).clamp(3.0, 12.0);
    let group_width = bar * groups + 2.0 * (groups - 1.0);
    let baseline = plot.bottom() - chart.fraction(0.0) as f32 * plot.height();
    let hovered = hovered_index(&response, plot, count);
    if let Some(index) = hovered {
        let x = plot.left() + band * index as f32;
        painter.rect_filled(
            Rect::from_min_max(Pos2::new(x, plot.top()), Pos2::new(x + band, plot.bottom())),
            0.0,
            ui.visuals().faint_bg_color.gamma_multiply(1.6),
        );
    }
    for (series_index, series) in chart.series.iter().enumerate() {
        let color = series_color(ui, series.slot);
        for (index, value) in series.values.iter().enumerate() {
            let Some(value) = value else { continue };
            if *value == 0.0 {
                continue;
            }
            let left = plot.left() + band * (index as f32 + 0.5) - group_width / 2.0
                + series_index as f32 * (bar + 2.0);
            let top = plot.bottom() - chart.fraction(*value) as f32 * plot.height();
            let (y0, y1) = if top < baseline {
                (top, baseline)
            } else {
                (baseline, top)
            };
            let radius = if top < baseline {
                CornerRadius {
                    nw: 4,
                    ne: 4,
                    sw: 0,
                    se: 0,
                }
            } else {
                CornerRadius {
                    nw: 0,
                    ne: 0,
                    sw: 4,
                    se: 4,
                }
            };
            painter.rect_filled(
                Rect::from_min_max(Pos2::new(left, y0), Pos2::new(left + bar, y1.max(y0 + 1.0))),
                radius,
                color,
            );
        }
    }
    if let Some(index) = hovered {
        tooltip(ui, &response, chart, index);
    }
}

/// Línea de 2 px con punto final; los huecos (`None`) cortan la línea.
pub fn line_chart(ui: &mut Ui, chart: &Chart, height: f32) {
    let (Plot { rect, plot }, response) = frame(ui, chart, height);
    let painter = ui.painter_at(rect);
    let count = chart.labels.len().max(1);
    let band = plot.width() / count as f32;
    let surface = ui.visuals().panel_fill;
    let hovered = hovered_index(&response, plot, count);
    for series in &chart.series {
        let color = series_color(ui, series.slot);
        let points: Vec<Option<Pos2>> = series
            .values
            .iter()
            .enumerate()
            .map(|(index, value)| {
                value.map(|value| {
                    Pos2::new(
                        plot.left() + band * (index as f32 + 0.5),
                        plot.bottom() - chart.fraction(value) as f32 * plot.height(),
                    )
                })
            })
            .collect();
        for pair in points.windows(2) {
            if let [Some(a), Some(b)] = pair {
                painter.line_segment([*a, *b], Stroke::new(2.0, color));
            }
        }
        let marker = |index: usize| {
            if let Some(Some(point)) = points.get(index) {
                painter.circle_filled(*point, 6.0, surface);
                painter.circle_filled(*point, 4.0, color);
            }
        };
        if let Some(last) = points.iter().rposition(Option::is_some) {
            marker(last);
        }
        if let Some(index) = hovered {
            let x = plot.left() + band * (index as f32 + 0.5);
            painter.line_segment(
                [Pos2::new(x, plot.top()), Pos2::new(x, plot.bottom())],
                Stroke::new(1.0, ui.visuals().weak_text_color()),
            );
            marker(index);
        }
    }
    if let Some(index) = hovered {
        tooltip(ui, &response, chart, index);
    }
}

/// Barra horizontal de proporción (categorías, presupuestos, metas).
pub fn ratio_bar(ui: &mut Ui, fraction: f32, color: Color32) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, 8.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(4), ui.visuals().extreme_bg_color);
    if fraction > 0.0 {
        let filled = Rect::from_min_size(
            rect.min,
            vec2(
                (rect.width() * fraction.clamp(0.0, 1.0)).max(4.0),
                rect.height(),
            ),
        );
        painter.rect_filled(filled, CornerRadius::same(4), color);
    }
}
