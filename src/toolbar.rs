//! Floating toolbar. One row that never changes order: its elements fold in and
//! out (width and opacity animated) to fit the window, and a palette row opens
//! above it when there is room.
//!
//! As the window grows: the slider lengthens, then Undo/Clear switch from icons
//! to text, then the slider reaches full length and the palette row opens.

use eframe::egui::{
    self, pos2, vec2, Align, Align2, Color32, Context, FontId, Frame, Id, Key, Layout, Margin,
    Rect, Response, Rounding, Sense, Stroke, Ui, UiBuilder,
};

use crate::i18n::t;
use crate::model::{Brush, Tool};
use crate::theme::{self, Colors};
use crate::widgets::{self, Icon};

/// Actions the toolbar can request from the app.
pub enum Action {
    OpenProject,
    SaveProject,
    Undo,
    Clear,
    Settings,
    Export,
}

/// Duration of every fold animation, in seconds.
const ANIM: f32 = 0.2;
const ROW_H: f32 = 32.0;
const PALETTE_H: f32 = 30.0;
const ROW_GAP: f32 = 4.0;
const PAD_X: f32 = 16.0;
const PAD_Y: f32 = 10.0;
/// Space kept between the toolbar and the window edges.
const EDGE: f32 = 8.0;
const COLOR_W: f32 = 28.0;
const PX_W: f32 = 38.0;
/// Two segments of 64 px plus 3 px of padding on each side.
const TOOL_W: f32 = 134.0;
const ICON_W: f32 = 32.0;
const SWATCH_W: f32 = 20.0;
const SLIDER_MIN: f32 = 32.0;
const SLIDER_MAX: f32 = 120.0;
/// Slider length needed before Undo and Clear switch to text.
const LABELS_SLIDER: f32 = 80.0;

/// Width of a toolbar text button: label plus 20 px of padding.
fn text_button_w(ctx: &Context, label: &str) -> f32 {
    ctx.fonts(|f| {
        f.layout_no_wrap(label.to_owned(), FontId::proportional(13.0), Color32::WHITE)
            .size()
            .x
    }) + 20.0
}

/// Width of the main row. `labels` goes from 0 (icons) to 1 (text); cells that are
/// folding count for their current share, so the row always matches what is drawn.
fn main_width(g: f32, labels: f32, undo_w: f32, clear_w: f32, slider: f32) -> f32 {
    let icons = 1.0 - labels;
    (COLOR_W + g)
        + (slider + g)
        + (PX_W + g)
        + (TOOL_W + g)
        + (1.0 + g)
        + 2.0 * (ICON_W + g) * icons
        + (undo_w + g + clear_w + g) * labels
        + (1.0 + g) * labels
        + (ICON_W + g)
        + ICON_W
}

/// Draws the toolbar at the bottom of the window and returns the action clicked
/// this frame, if any.
pub fn show(
    ctx: &Context,
    brush: &mut Brush,
    palette: &mut [Color32; 6],
    c: &Colors,
) -> Option<Action> {
    let screen_w = ctx.screen_rect().width();
    // Gap between elements: 4 px at the minimum window width, 10 px from 700 px.
    let g = 4.0 + 6.0 * ((screen_w - 480.0) / 220.0).clamp(0.0, 1.0);
    let avail = (screen_w - 2.0 * EDGE - 2.0 * PAD_X - 2.0).max(0.0);

    let undo_w = text_button_w(ctx, t("action.undo"));
    let clear_w = text_button_w(ctx, t("action.clear"));

    let labels_on = avail >= main_width(g, 1.0, undo_w, clear_w, LABELS_SLIDER);
    let palette_on = avail >= main_width(g, 1.0, undo_w, clear_w, SLIDER_MAX);
    let tl = ctx.animate_bool_with_time(Id::new("toolbar_labels"), labels_on, ANIM);
    let tp = ctx.animate_bool_with_time(Id::new("toolbar_palette"), palette_on, ANIM);

    // The slider takes what the other elements leave, so the row always fits.
    let slider_w = (avail - main_width(g, tl, undo_w, clear_w, 0.0)).clamp(SLIDER_MIN, SLIDER_MAX);
    let row_w = main_width(g, tl, undo_w, clear_w, slider_w);
    let palette_w = 6.0 * SWATCH_W + 5.0 * g;
    let total_h = ROW_H + (PALETTE_H + ROW_GAP) * tp;

    let mut action = None;

    egui::Area::new(Id::new("toolbar"))
        .anchor(Align2::CENTER_BOTTOM, vec2(0.0, -24.0))
        .show(ctx, |ui| {
            theme::apply(ui, c);

            Frame::none()
                .fill(c.surface)
                .stroke(Stroke::new(1.0_f32, c.border))
                .rounding(Rounding::same(14.0))
                .inner_margin(Margin::symmetric(PAD_X, PAD_Y))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                    ui.spacing_mut().interact_size = vec2(32.0, 32.0);

                    ui.allocate_ui_with_layout(
                        vec2(row_w, total_h),
                        Layout::top_down(Align::Center),
                        |ui| {
                            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                            if tp > 0.001 {
                                palette_row(ui, brush, palette, c, g, tp, palette_w);
                            }
                            ui.allocate_ui_with_layout(
                                vec2(row_w, ROW_H),
                                Layout::left_to_right(Align::Center),
                                |ui| {
                                    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                                    main_row(
                                        ui,
                                        brush,
                                        c,
                                        &mut action,
                                        g,
                                        tl,
                                        slider_w,
                                        undo_w,
                                        clear_w,
                                    );
                                },
                            );
                        },
                    );
                });
        });

    action
}

/// One element of the row. Its slot is `(w + gap) * t` wide and the content is
/// clipped to it with opacity `t`, so it grows or shrinks smoothly.
fn cell(ui: &mut Ui, key: &str, t: f32, w: f32, gap: f32, add: impl FnOnce(&mut Ui)) {
    if t <= 0.001 {
        return;
    }
    let (slot, _) = ui.allocate_exact_size(vec2((w + gap) * t, ROW_H), Sense::hover());
    let mut child = ui.new_child(
        UiBuilder::new()
            .id_salt(key)
            .max_rect(Rect::from_min_size(slot.min, vec2(w, ROW_H)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    // 2 px of slack, so outlines drawn on the edge of a control are not cut.
    child.set_clip_rect(slot.expand(2.0).intersect(ui.clip_rect()));
    child.spacing_mut().item_spacing.x = 0.0;
    if t < 1.0 {
        child.multiply_opacity(t);
    }
    add(&mut child);
}

#[allow(clippy::too_many_arguments)]
fn main_row(
    ui: &mut Ui,
    brush: &mut Brush,
    c: &Colors,
    action: &mut Option<Action>,
    g: f32,
    tl: f32,
    slider_w: f32,
    undo_w: f32,
    clear_w: f32,
) {
    let eraser = brush.tool == Tool::Eraser;
    let icons = 1.0 - tl;

    cell(ui, "color", 1.0, COLOR_W, g, |ui| {
        let before = brush.color;
        widgets::color_button(ui, "toolbar_color", c, &mut brush.color, !eraser);
        if !eraser && brush.color != before {
            brush.tool = Tool::Brush;
        }
    });
    cell(ui, "slider", 1.0, slider_w, g, |ui| {
        widgets::size_slider(
            ui,
            c,
            &mut brush.width,
            1.0..=80.0,
            slider_w,
            t("toolbar.size"),
        );
    });
    cell(ui, "px", 1.0, PX_W, g, |ui| {
        widgets::px_label(ui, c, &format!("{:.0} px", brush.width));
    });
    cell(ui, "tool", 1.0, TOOL_W, g, |ui| {
        tool_dropdown(ui, brush, c);
    });
    cell(ui, "div1", 1.0, 1.0, g, |ui| widgets::divider(ui, c));

    cell(ui, "undo_icon", icons, ICON_W, g, |ui| {
        icon_action(
            ui,
            "toolbar_undo",
            c,
            Icon::Undo,
            t("action.undo"),
            &mut *action,
            Action::Undo,
        )
    });
    cell(ui, "clear_icon", icons, ICON_W, g, |ui| {
        icon_action(
            ui,
            "toolbar_clear",
            c,
            Icon::Trash,
            t("action.clear"),
            &mut *action,
            Action::Clear,
        )
    });
    cell(ui, "undo_text", tl, undo_w, g, |ui| {
        if widgets::toolbar_button(ui, "toolbar_undo", c, t("action.undo")).clicked() {
            *action = Some(Action::Undo);
        }
    });
    cell(ui, "clear_text", tl, clear_w, g, |ui| {
        if widgets::toolbar_button(ui, "toolbar_clear", c, t("action.clear")).clicked() {
            *action = Some(Action::Clear);
        }
    });
    cell(ui, "div2", tl, 1.0, g, |ui| widgets::divider(ui, c));

    cell(ui, "export", 1.0, ICON_W, g, |ui| {
        if let Some(selected) = project_dropdown(ui, c) {
            *action = Some(selected);
        }
    });
    cell(ui, "settings", 1.0, ICON_W, 0.0, |ui| {
        icon_action(
            ui,
            "toolbar_settings",
            c,
            Icon::Settings,
            t("settings.title"),
            &mut *action,
            Action::Settings,
        )
    });
}

fn tool_dropdown(ui: &mut Ui, brush: &mut Brush, c: &Colors) {
    let response = dropdown_button(
        ui,
        "toolbar_tool",
        tool_name(brush.tool),
        brush.tool,
        c,
        TOOL_W - 8.0,
    );
    let ctx = ui.ctx().clone();
    let id = Id::new("toolbar_tool_menu_open");
    let mut open = ctx.data(|data| data.get_temp::<bool>(id)).unwrap_or(false);
    if response.clicked() {
        open = !open;
    }

    let mut selected_tool = None;
    let mut fill_changed = false;
    if open {
        let menu_rect = dropdown_panel(&ctx, response.rect, "toolbar_tool_menu", c, 224.0, |ui| {
            for (tool, key) in [
                (Tool::Brush, "tool.brush"),
                (Tool::Eraser, "tool.eraser"),
                (Tool::Line, "tool.line"),
                (Tool::Rectangle, "tool.rectangle"),
                (Tool::Ellipse, "tool.ellipse"),
                (Tool::Eyedropper, "tool.eyedropper"),
            ] {
                if dropdown_option(ui, c, t(key), Some(tool), brush.tool == tool, 214.0).clicked() {
                    selected_tool = Some(tool);
                }
            }
            ui.add_space(3.0);
            ui.separator();
            fill_changed = fill_shapes_button(ui, c, brush.fill_shapes).clicked();
            if fill_changed {
                brush.fill_shapes = !brush.fill_shapes;
            }
        });
        let escape = ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, Key::Escape));
        let outside_pressed = ctx.input(|input| {
            input.pointer.any_pressed()
                && input.pointer.interact_pos().is_some_and(|point| {
                    !menu_rect.contains(point) && !response.rect.contains(point)
                })
        });
        open = keep_dropdown_open(
            open,
            escape,
            selected_tool.is_some(),
            fill_changed,
            outside_pressed,
        );
    }
    if let Some(tool) = selected_tool {
        brush.tool = tool;
    }
    ctx.data_mut(|data| data.insert_temp(id, open));
}

fn project_dropdown(ui: &mut Ui, c: &Colors) -> Option<Action> {
    let response = widgets::icon_button(ui, "toolbar_export", c, Icon::Export, t("menu.files"));
    let ctx = ui.ctx().clone();
    let id = Id::new("toolbar_project_menu_open");
    let mut open = ctx.data(|data| data.get_temp::<bool>(id)).unwrap_or(false);
    if response.clicked() {
        open = !open;
    }

    let mut selected = None;
    if open {
        let menu_rect = dropdown_panel(
            &ctx,
            response.rect,
            "toolbar_project_menu",
            c,
            248.0,
            |ui| {
                for (label, action) in [
                    ("project.open", Action::OpenProject),
                    ("project.save", Action::SaveProject),
                    ("action.export", Action::Export),
                ] {
                    if dropdown_option(ui, c, t(label), None, false, 238.0).clicked() {
                        selected = Some(action);
                    }
                }
            },
        );
        let escape = ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, Key::Escape));
        let outside_pressed = ctx.input(|input| {
            input.pointer.any_pressed()
                && input.pointer.interact_pos().is_some_and(|point| {
                    !menu_rect.contains(point) && !response.rect.contains(point)
                })
        });
        open = keep_dropdown_open(open, escape, selected.is_some(), false, outside_pressed);
    }
    ctx.data_mut(|data| data.insert_temp(id, open));
    selected
}

fn keep_dropdown_open(
    open: bool,
    escape: bool,
    item_selected: bool,
    option_changed: bool,
    outside_pressed: bool,
) -> bool {
    open && !escape && !item_selected && !option_changed && !outside_pressed
}

fn dropdown_button(
    ui: &mut Ui,
    id: &str,
    label: &str,
    tool: Tool,
    c: &Colors,
    width: f32,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(width, ROW_H), Sense::click());
    let hover =
        ui.ctx()
            .animate_bool_with_time(Id::new(("dropdown_hover", id)), response.hovered(), 0.12);
    let fill = c.track.lerp_to_gamma(c.hover, hover);
    let painter = ui.painter();
    painter.rect_filled(rect, Rounding::same(9.0), fill);
    widgets::tool_icon(
        ui,
        tool,
        c,
        Rect::from_center_size(pos2(rect.left() + 18.0, rect.center().y), vec2(18.0, 18.0)),
    );
    painter.text(
        pos2(rect.left() + 34.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(13.0),
        c.text,
    );
    let center = pos2(rect.right() - 13.0, rect.center().y);
    let stroke = Stroke::new(1.5_f32, c.text_dim);
    painter.line_segment([center + vec2(-3.0, -1.5), center + vec2(0.0, 1.5)], stroke);
    painter.line_segment([center + vec2(0.0, 1.5), center + vec2(3.0, -1.5)], stroke);
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    response
}

fn dropdown_panel(
    ctx: &Context,
    anchor: Rect,
    id: &str,
    c: &Colors,
    width: f32,
    add_contents: impl FnOnce(&mut Ui),
) -> Rect {
    egui::Area::new(Id::new(id))
        .order(egui::Order::Foreground)
        .pivot(Align2::RIGHT_BOTTOM)
        .fixed_pos(pos2(anchor.right(), anchor.top() - 8.0))
        .show(ctx, |ui| {
            Frame::none()
                .fill(c.surface)
                .stroke(Stroke::new(1.0_f32, c.border))
                .rounding(Rounding::same(12.0))
                .inner_margin(Margin::same(5.0))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = vec2(0.0, 2.0);
                    ui.set_width(width - 10.0);
                    add_contents(ui);
                });
        })
        .response
        .rect
}

fn dropdown_option(
    ui: &mut Ui,
    c: &Colors,
    label: &str,
    tool: Option<Tool>,
    selected: bool,
    width: f32,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(width, 32.0), Sense::click());
    let painter = ui.painter();
    let background = if response.hovered() {
        c.hover
    } else if selected {
        c.active
    } else {
        c.surface
    };
    painter.rect_filled(rect, Rounding::same(7.0), background);
    if let Some(tool) = tool {
        widgets::tool_icon(
            ui,
            tool,
            c,
            Rect::from_center_size(pos2(rect.left() + 17.0, rect.center().y), vec2(18.0, 18.0)),
        );
    } else if selected {
        painter.circle_filled(pos2(rect.left() + 14.0, rect.center().y), 3.0, c.accent);
    }
    painter.text(
        pos2(
            rect.left() + if tool.is_some() { 34.0 } else { 28.0 },
            rect.center().y,
        ),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(13.0),
        c.text,
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, label)
    });
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

fn fill_shapes_button(ui: &mut Ui, c: &Colors, filled: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(214.0, 36.0), Sense::click());
    let hover =
        ui.ctx()
            .animate_bool_with_time(Id::new("fill_shapes_hover"), response.hovered(), 0.12);
    let background = if filled { c.active } else { c.surface };
    ui.painter().rect_filled(
        rect,
        Rounding::same(8.0),
        background.lerp_to_gamma(c.hover, hover),
    );
    widgets::tool_icon(
        ui,
        Tool::Rectangle,
        c,
        Rect::from_center_size(pos2(rect.left() + 18.0, rect.center().y), vec2(18.0, 18.0)),
    );
    ui.painter().text(
        pos2(rect.left() + 34.0, rect.center().y),
        Align2::LEFT_CENTER,
        t("tool.fill"),
        FontId::proportional(13.0),
        c.text,
    );

    let toggle =
        Rect::from_center_size(pos2(rect.right() - 18.0, rect.center().y), vec2(18.0, 18.0));
    if filled {
        ui.painter()
            .rect_filled(toggle, Rounding::same(5.0), c.accent);
        let check = c.primary_fg;
        ui.painter().line_segment(
            [
                pos2(toggle.left() + 4.0, toggle.center().y),
                pos2(toggle.left() + 7.0, toggle.center().y + 3.0),
            ],
            Stroke::new(1.7_f32, check),
        );
        ui.painter().line_segment(
            [
                pos2(toggle.left() + 7.0, toggle.center().y + 3.0),
                pos2(toggle.right() - 3.0, toggle.top() + 4.0),
            ],
            Stroke::new(1.7_f32, check),
        );
    } else {
        ui.painter().rect_stroke(
            toggle,
            Rounding::same(5.0),
            Stroke::new(1.2_f32, c.text_dim),
        );
    }
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, filled, t("tool.fill"))
    });
    response
}

fn icon_action(
    ui: &mut Ui,
    id: &str,
    c: &Colors,
    icon: Icon,
    tip: &str,
    action: &mut Option<Action>,
    value: Action,
) {
    if widgets::icon_button(ui, id, c, icon, tip).clicked() {
        *action = Some(value);
    }
}

/// The six ink colours. The row opens above the main row with height `tp`.
/// While the eraser is active the swatches are dimmed, like the colour button.
fn palette_row(
    ui: &mut Ui,
    brush: &mut Brush,
    palette: &mut [Color32; 6],
    c: &Colors,
    g: f32,
    tp: f32,
    width: f32,
) {
    let enabled = brush.tool != Tool::Eraser;
    let (slot, _) = ui.allocate_exact_size(vec2(width, PALETTE_H * tp), Sense::hover());
    let mut row = ui.new_child(
        UiBuilder::new()
            .id_salt("palette")
            .max_rect(Rect::from_min_size(slot.min, vec2(width, PALETTE_H)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    // A little extra room on the sides, so the selection ring is not cut.
    row.set_clip_rect(slot.expand2(vec2(8.0, 0.0)).intersect(ui.clip_rect()));
    row.spacing_mut().item_spacing.x = 0.0;
    if tp < 1.0 {
        row.multiply_opacity(tp);
    }

    let mut next = None;
    let palette_len = palette.len();
    for (i, color) in palette.iter_mut().enumerate() {
        let selected = enabled && brush.color == *color;
        let resp = widgets::palette_swatch(&mut row, i, color, selected, enabled, c);
        if resp.clicked() {
            brush.color = *color;
            brush.tool = Tool::Brush;
        }
        if resp.has_focus() {
            let (left, right) = row.input(|input| {
                (
                    input.key_pressed(Key::ArrowLeft),
                    input.key_pressed(Key::ArrowRight),
                )
            });
            if left || right {
                let step = if left { palette_len - 1 } else { 1 };
                next = Some((i + step) % palette_len);
            }
        }
        if i + 1 < palette_len {
            row.add_space(g);
        }
    }
    if let Some(index) = next {
        brush.color = palette[index];
        brush.tool = Tool::Brush;
        row.memory_mut(|m| m.request_focus(Id::new(("palette_swatch", index))));
    }

    ui.add_space(ROW_GAP * tp);
}

fn tool_name(tool: Tool) -> &'static str {
    t(match tool {
        Tool::Brush => "tool.brush",
        Tool::Eraser => "tool.eraser",
        Tool::Line => "tool.line",
        Tool::Rectangle => "tool.rectangle",
        Tool::Ellipse => "tool.ellipse",
        Tool::Eyedropper => "tool.eyedropper",
    })
}

#[cfg(test)]
mod tests {
    use super::keep_dropdown_open;

    #[test]
    fn dropdown_stays_open_until_dismissed_or_an_option_is_used() {
        assert!(keep_dropdown_open(true, false, false, false, false));
        assert!(!keep_dropdown_open(true, true, false, false, false));
        assert!(!keep_dropdown_open(true, false, true, false, false));
        assert!(!keep_dropdown_open(true, false, false, true, false));
        assert!(!keep_dropdown_open(true, false, false, false, true));
    }

    #[test]
    fn closed_dropdown_does_not_open_without_a_button_click() {
        assert!(!keep_dropdown_open(false, false, false, false, false));
    }
}
