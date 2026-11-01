use eframe::egui::{
    self, vec2, Align2, Color32, Context, Frame, Id, Key, Margin, Order, Rounding, ScrollArea,
    Sense, Stroke, Ui,
};

use crate::theme::{self, Colors};

/// Space kept between the popup and the window edges.
const EDGE_MARGIN: f32 = 16.0;
/// Inner padding of the popup, on each side.
const INNER_PADDING: f32 = 24.0;
/// Corner radius of the popup card.
const ROUNDING: f32 = 16.0;

/// True while the dialog is on screen, including its fade-out.
/// Call it after `show` has run for the current frame.
pub fn is_visible(ctx: &Context, id: &str) -> bool {
    ctx.data(|d| d.get_temp::<f32>(Id::new(("dialog_visibility", id))))
        .is_some_and(|t| t > 0.0)
}

/// Animated modal popup with a dimmed background and a slide-in effect.
/// `width` is the outer width of the card, padding included. It is reduced if
/// the window is too narrow, and the popup is always kept inside the window.
/// `fill` is the card colour: `surface_alt` for the settings card, `surface`
/// for the other popups.
/// Returns `true` when the user asks to close it (click on the background or Escape).
pub fn show(
    ctx: &Context,
    id: &str,
    open: bool,
    c: &Colors,
    width: f32,
    fill: Color32,
    content: impl FnOnce(&mut Ui),
) -> bool {
    // Each opening gets a new session number. egui keeps the size of an area and of a
    // scroll area between frames; a new id forces them to be measured again, so a
    // reopened popup gets its real size instead of the one from its previous opening.
    let was_open_id = Id::new(("dialog_was_open", id));
    let session_id = Id::new(("dialog_session", id));
    let was_open = ctx
        .data(|d| d.get_temp::<bool>(was_open_id))
        .unwrap_or(false);
    if open && !was_open {
        ctx.data_mut(|d| {
            let next = d.get_temp::<u32>(session_id).unwrap_or(0) + 1;
            d.insert_temp(session_id, next);
        });
    }
    ctx.data_mut(|d| d.insert_temp(was_open_id, open));
    let session = ctx.data(|d| d.get_temp::<u32>(session_id)).unwrap_or(0);

    // Fade and slide progress, from 0.0 (closed) to 1.0 (open).
    let t = ctx.animate_bool_with_time(Id::new(("dialog_anim", id)), open, 0.16);
    // Remembered so callers can check whether another dialog is still on screen.
    ctx.data_mut(|d| d.insert_temp(Id::new(("dialog_visibility", id)), t));
    if t <= 0.0 {
        return false;
    }

    let screen = ctx.screen_rect();
    let mut dismissed = false;

    // Dimmed background. A click on it closes the dialog.
    egui::Area::new(Id::new(("dialog_scrim", id)))
        .order(Order::Foreground)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            let (rect, resp) = ui.allocate_exact_size(screen.size(), Sense::click_and_drag());
            let scrim = c.scrim();
            let alpha = (scrim.a() as f32 * t) as u8;
            ui.painter()
                .rect_filled(rect, 0.0, Color32::from_black_alpha(alpha));
            if open && resp.clicked() {
                dismissed = true;
            }
        });

    // Never wider than the window allows.
    let max_w = (screen.width() - 2.0 * EDGE_MARGIN).max(120.0);
    let width = width.min(max_w);
    let content_w = (width - 2.0 * INNER_PADDING).max(72.0);
    // The popup must fit between the two margins, padding included.
    let max_h = (screen.height() - 2.0 * EDGE_MARGIN).max(120.0);
    let max_content_h = (max_h - 2.0 * INNER_PADDING).max(40.0);

    egui::Area::new(Id::new(("dialog_box", id, session)))
        .order(Order::Tooltip)
        .anchor(Align2::CENTER_CENTER, vec2(0.0, (1.0 - t) * 14.0))
        // Measure the content against the full height available, not a default size:
        // the popup then shrinks to its content or scrolls when it is too tall.
        .default_size(vec2(width, max_h))
        .show(ctx, |ui| {
            theme::apply(ui, c);
            Frame::none()
                .fill(fill)
                .stroke(Stroke::new(1.0_f32, c.border))
                .rounding(Rounding::same(ROUNDING))
                .inner_margin(Margin::same(INNER_PADDING))
                .show(ui, |ui| {
                    // The popup is exactly as wide as its content, which never exceeds the window.
                    ui.set_width(content_w);
                    // The height is decided here, from the content size measured on the
                    // previous frame, never from the size egui remembers for the area.
                    // That remembered size would keep the popup small after the window
                    // grows back. The height eases toward its target, so a resize is smooth.
                    // Scrolls only when the window is too short for the content.
                    let height_id = Id::new(("dialog_content_h", id, session));
                    let measured = ctx.data(|d| d.get_temp::<f32>(height_id));
                    let target = measured.map_or(max_content_h, |h| h.min(max_content_h));
                    let height = if measured.is_some() {
                        ctx.animate_value_with_time(
                            Id::new(("dialog_h_anim", id, session)),
                            target,
                            0.12,
                        )
                    } else {
                        target
                    };
                    let out = ui.allocate_ui_with_layout(
                        vec2(content_w, height),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ScrollArea::vertical()
                                .id_salt(("dialog_scroll", id, session))
                                .max_height(height)
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    content(ui);
                                })
                        },
                    );
                    let content_h = out.inner.content_size.y;
                    ctx.data_mut(|d| d.insert_temp(height_id, content_h));
                });
        });

    if open && ctx.input(|i| i.key_pressed(Key::Escape)) {
        dismissed = true;
    }

    dismissed
}
