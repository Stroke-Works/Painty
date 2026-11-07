//! The window frame: the floating window controls, the rounded corners and the edges
//! used to resize. The native frame is turned off, so all is drawn here.

use eframe::egui::{
    vec2, Area, Color32, Context, CursorIcon, Id, Order, Painter, Pos2, Rect, ResizeDirection,
    Rounding, Sense, Stroke, ViewportCommand,
};

use crate::theme::Colors;

/// Corner radius of the window.
const RADIUS: f32 = 10.0;
/// Size of the floating window-control rectangle.
const BUTTON_H: f32 = 26.0;
const BUTTON_W: f32 = 34.0;
const GRIP_W: f32 = 22.0;
/// Inner padding of the pill around the buttons.
const PAD: f32 = 3.0;
const CONTROLS_ROUNDING: f32 = 10.0;
const BUTTON_ROUNDING: f32 = 7.0;
/// Distance between the floating controls and the window edges.
const CONTROLS_MARGIN: f32 = 8.0;
/// Width of the strip along each edge that starts a resize.
const EDGE: f32 = 5.0;
/// Size of the square at each corner that starts a diagonal resize.
const CORNER: f32 = 12.0;
/// Background of the close button on hover, as on Windows.
const CLOSE_HOVER: Color32 = Color32::from_rgb(0xe8, 0x11, 0x23);
/// Background of the close button while pressed.
const CLOSE_PRESSED: Color32 = Color32::from_rgb(0xf1, 0x70, 0x7a);

#[derive(Clone, Copy, PartialEq, Eq)]
enum WindowButton {
    Move,
    Minimize,
    Maximize,
    Close,
}

fn is_maximized(ctx: &Context) -> bool {
    ctx.input(|i| i.viewport().maximized.unwrap_or(false))
}

/// Corner radius to use for the window shape: square when it fills the screen.
fn radius(ctx: &Context) -> f32 {
    if is_maximized(ctx) {
        0.0
    } else {
        RADIUS
    }
}

/// Shape of the whole window, for backgrounds that cover it.
pub fn window_rounding(ctx: &Context) -> Rounding {
    Rounding::same(radius(ctx))
}

/// Rounds a position to the middle of a physical pixel, so one point wide lines are crisp.
fn crisp(ctx: &Context, v: f32) -> f32 {
    let ppp = ctx.pixels_per_point();
    ((v * ppp).floor() + 0.5) / ppp
}

/// Floating window controls in the top-right corner: a small pill holding a drag grip and
/// three buttons. Dragging the grip or the pill edge moves the window, and a double click
/// maximizes it.
pub fn show(ctx: &Context, c: &Colors) {
    let maximized = is_maximized(ctx);
    let buttons = [
        WindowButton::Move,
        WindowButton::Minimize,
        WindowButton::Maximize,
        WindowButton::Close,
    ];
    let width_of = |k: WindowButton| {
        if k == WindowButton::Move {
            GRIP_W
        } else {
            BUTTON_W
        }
    };
    let total_w = PAD * 2.0 + buttons.iter().map(|k| width_of(*k)).sum::<f32>();
    let bar_size = vec2(total_w, BUTTON_H + PAD * 2.0);
    let pos = Pos2::new(
        ctx.screen_rect().right() - CONTROLS_MARGIN - bar_size.x,
        ctx.screen_rect().top() + CONTROLS_MARGIN,
    );
    Area::new(Id::new("window_controls"))
        .order(Order::Foreground)
        .fixed_pos(pos)
        .show(ctx, |ui| {
            let (bar, drag) = ui.allocate_exact_size(bar_size, Sense::click_and_drag());
            if drag.double_clicked() {
                ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
            } else if drag.drag_started() {
                ctx.send_viewport_cmd(ViewportCommand::StartDrag);
            }

            let painter = ui.painter().clone();
            painter.rect_filled(bar, CONTROLS_ROUNDING, c.surface);
            painter.rect_stroke(bar, CONTROLS_ROUNDING, Stroke::new(1.0_f32, c.border));

            let mut left = bar.left() + PAD;
            for (i, kind) in buttons.into_iter().enumerate() {
                let w = width_of(kind);
                let rect = Rect::from_min_size(Pos2::new(left, bar.top() + PAD), vec2(w, BUTTON_H));
                left += w;
                let sense = match kind {
                    WindowButton::Move => Sense::click_and_drag(),
                    _ => Sense::click(),
                };
                let response = ui.interact(rect, Id::new(("window_button", i)), sense);
                let close = kind == WindowButton::Close;
                if kind == WindowButton::Move && response.hovered() {
                    ctx.set_cursor_icon(CursorIcon::Grab);
                }
                let hovered = response.hovered();
                let down = response.is_pointer_button_down_on();
                let (bg, fg) = match (close, hovered, down) {
                    (true, true, true) => (CLOSE_PRESSED, Color32::WHITE),
                    (true, true, false) => (CLOSE_HOVER, Color32::WHITE),
                    (false, true, true) => (c.active, c.text),
                    (false, true, false) => (c.hover, c.text),
                    _ if kind == WindowButton::Move => (c.surface, c.text_dim),
                    _ => (c.surface, c.text),
                };
                if hovered {
                    painter.rect_filled(rect, BUTTON_ROUNDING, bg);
                }
                draw_glyph(ctx, &painter, rect.center(), kind, maximized, fg, bg);

                // Thin separator between the grip and the buttons.
                if kind == WindowButton::Move {
                    let sx = crisp(ctx, rect.right() + 2.0);
                    painter.line_segment(
                        [
                            Pos2::new(sx, rect.top() + 5.0),
                            Pos2::new(sx, rect.bottom() - 5.0),
                        ],
                        Stroke::new(1.0_f32, c.separator),
                    );
                    left += 4.0;
                }

                if kind == WindowButton::Move {
                    if response.double_clicked() {
                        ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
                    } else if response.drag_started() {
                        ctx.send_viewport_cmd(ViewportCommand::StartDrag);
                    }
                } else if response.clicked() {
                    let command = match kind {
                        WindowButton::Move => None,
                        WindowButton::Minimize => Some(ViewportCommand::Minimized(true)),
                        WindowButton::Maximize => Some(ViewportCommand::Maximized(!maximized)),
                        WindowButton::Close => Some(ViewportCommand::Close),
                    };
                    if let Some(command) = command {
                        ctx.send_viewport_cmd(command);
                    }
                }
            }
        });
}

/// Draws a line with round caps, so small icons stay smooth at any scale.
fn round_line(painter: &Painter, a: Pos2, b: Pos2, stroke: Stroke) {
    painter.line_segment([a, b], stroke);
    let radius = stroke.width * 0.5;
    painter.circle_filled(a, radius, stroke.color);
    painter.circle_filled(b, radius, stroke.color);
}

/// Draws the icon of a window button, centred on `center`. `bg` is what lies behind it.
fn draw_glyph(
    ctx: &Context,
    painter: &Painter,
    center: Pos2,
    kind: WindowButton,
    maximized: bool,
    color: Color32,
    bg: Color32,
) {
    let stroke = Stroke::new(1.3_f32, color);
    let (x, y) = (crisp(ctx, center.x), crisp(ctx, center.y));
    let at = |dx: f32, dy: f32| Pos2::new(x + dx, y + dy);
    let half = 4.5;
    match kind {
        WindowButton::Move => {
            // Grip: two columns of three dots.
            for dx in [-2.5_f32, 2.5] {
                for dy in [-5.0_f32, 0.0, 5.0] {
                    painter.circle_filled(at(dx, dy), 1.1, color);
                }
            }
        }
        WindowButton::Minimize => {
            round_line(painter, at(-half, 0.0), at(half, 0.0), stroke);
        }
        WindowButton::Maximize if maximized => {
            let size = 7.0;
            let offset = 2.0;
            let o = (size + offset) / 2.0;
            let back = Rect::from_min_size(at(-o + offset, -o), vec2(size, size));
            painter.rect_stroke(back, 1.5, stroke);
            let front = Rect::from_min_size(at(-o, -o + offset), vec2(size, size));
            painter.rect_filled(front, 1.5, bg);
            painter.rect_stroke(front, 1.5, stroke);
        }
        WindowButton::Maximize => {
            let square = Rect::from_center_size(at(0.0, 0.0), vec2(2.0 * half, 2.0 * half));
            painter.rect_stroke(square, 1.5, stroke);
        }
        WindowButton::Close => {
            round_line(painter, at(-half, -half), at(half, half), stroke);
            round_line(painter, at(half, -half), at(-half, half), stroke);
        }
    }
}

/// Invisible strips along the edges and corners that start a resize. They sit above the
/// canvas, so a press on the edge never reaches it and does not draw.
pub fn resize_handles(ctx: &Context) {
    if is_maximized(ctx) {
        return;
    }
    let screen = ctx.screen_rect();
    let (l, t, r, b) = (screen.left(), screen.top(), screen.right(), screen.bottom());
    let strip = |min: Pos2, max: Pos2| Rect::from_min_max(min, max);
    use CursorIcon::{ResizeHorizontal, ResizeNeSw, ResizeNwSe, ResizeVertical};
    use ResizeDirection as D;
    // Edges first, corners last: the corners are on top where they overlap.
    let handles = [
        (
            "n",
            strip(Pos2::new(l + CORNER, t), Pos2::new(r - CORNER, t + EDGE)),
            D::North,
            ResizeVertical,
        ),
        (
            "s",
            strip(Pos2::new(l + CORNER, b - EDGE), Pos2::new(r - CORNER, b)),
            D::South,
            ResizeVertical,
        ),
        (
            "w",
            strip(Pos2::new(l, t + CORNER), Pos2::new(l + EDGE, b - CORNER)),
            D::West,
            ResizeHorizontal,
        ),
        (
            "e",
            strip(Pos2::new(r - EDGE, t + CORNER), Pos2::new(r, b - CORNER)),
            D::East,
            ResizeHorizontal,
        ),
        (
            "nw",
            strip(Pos2::new(l, t), Pos2::new(l + CORNER, t + CORNER)),
            D::NorthWest,
            ResizeNwSe,
        ),
        (
            "ne",
            strip(Pos2::new(r - CORNER, t), Pos2::new(r, t + CORNER)),
            D::NorthEast,
            ResizeNeSw,
        ),
        (
            "sw",
            strip(Pos2::new(l, b - CORNER), Pos2::new(l + CORNER, b)),
            D::SouthWest,
            ResizeNeSw,
        ),
        (
            "se",
            strip(Pos2::new(r - CORNER, b - CORNER), Pos2::new(r, b)),
            D::SouthEast,
            ResizeNwSe,
        ),
    ];
    for (name, rect, direction, cursor) in handles {
        Area::new(Id::new(("resize_handle", name)))
            .order(Order::Foreground)
            .fixed_pos(rect.min)
            .show(ctx, |ui| {
                let (_, response) = ui.allocate_exact_size(rect.size(), Sense::drag());
                if response.hovered() {
                    ctx.set_cursor_icon(cursor);
                    if ctx.input(|i| i.pointer.primary_pressed()) {
                        ctx.send_viewport_cmd(ViewportCommand::BeginResize(direction));
                    }
                }
            });
    }
}
