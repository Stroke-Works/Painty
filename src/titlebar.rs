//! The window frame: the floating window controls, the rounded corners, the border
//! and the edges used to resize. The native frame is turned off, so all is drawn here.

use eframe::egui::{
    vec2, Area, Color32, Context, CursorIcon, Id, Order, Painter, Pos2, Rect, ResizeDirection,
    Rounding, Sense, Stroke, ViewportCommand,
};

use crate::theme::Colors;

/// Corner radius of the window.
const RADIUS: f32 = 10.0;
/// Size of the floating window-control rectangle.
const HEIGHT: f32 = 30.0;
const BUTTON_W: f32 = 40.0;
const CONTROLS_ROUNDING: f32 = 7.0;
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

/// Floating window controls in the top-right corner. The rest of that strip is the
/// canvas. Dragging the rectangle moves the window and a double click maximizes it.
pub fn show(ctx: &Context, c: &Colors) {
    let maximized = is_maximized(ctx);
    let buttons = [
        WindowButton::Minimize,
        WindowButton::Maximize,
        WindowButton::Close,
    ];
    let bar_size = vec2(BUTTON_W * buttons.len() as f32, HEIGHT);
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

            for (i, kind) in buttons.into_iter().enumerate() {
                let left = bar.right() - BUTTON_W * (buttons.len() - i) as f32;
                let rect = Rect::from_min_size(Pos2::new(left, bar.top()), vec2(BUTTON_W, HEIGHT));
                let response = ui.interact(rect, Id::new(("window_button", i)), Sense::click());
                let close = kind == WindowButton::Close;
                let (bg, fg) = match (
                    close,
                    response.hovered(),
                    response.is_pointer_button_down_on(),
                ) {
                    (true, true, true) => (CLOSE_PRESSED, Color32::WHITE),
                    (true, true, false) => (CLOSE_HOVER, Color32::WHITE),
                    (false, true, true) => (c.active, c.text),
                    (false, true, false) => (c.hover, c.text),
                    _ => (c.surface, c.text),
                };
                // The outer buttons follow the rounded corners of the rectangle.
                let rounding = match i {
                    0 => Rounding {
                        nw: CONTROLS_ROUNDING,
                        sw: CONTROLS_ROUNDING,
                        ..Rounding::ZERO
                    },
                    2 => Rounding {
                        ne: CONTROLS_ROUNDING,
                        se: CONTROLS_ROUNDING,
                        ..Rounding::ZERO
                    },
                    _ => Rounding::ZERO,
                };
                painter.rect_filled(rect, rounding, bg);
                draw_glyph(ctx, &painter, rect.center(), kind, maximized, fg, bg);

                if response.clicked() {
                    ctx.send_viewport_cmd(match kind {
                        WindowButton::Minimize => ViewportCommand::Minimized(true),
                        WindowButton::Maximize => ViewportCommand::Maximized(!maximized),
                        WindowButton::Close => ViewportCommand::Close,
                    });
                }
            }

            painter.rect_stroke(bar, CONTROLS_ROUNDING, Stroke::new(1.0_f32, c.border));
        });
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
    let stroke = Stroke::new(1.0_f32, color);
    let half = 5.0;
    // A square whose lines fall in the middle of pixels.
    let square = |left: f32, top: f32| {
        let min = Pos2::new(crisp(ctx, left), crisp(ctx, top));
        Rect::from_min_size(min, vec2(2.0 * half, 2.0 * half))
    };
    match kind {
        WindowButton::Minimize => {
            let y = crisp(ctx, center.y);
            painter.hline(center.x - half..=center.x + half, y, stroke);
        }
        WindowButton::Maximize if maximized => {
            let (x, y) = (center.x - half, center.y - half);
            painter.rect_stroke(square(x + 2.0, y - 2.0), 0.0, stroke);
            let front = square(x - 1.0, y + 1.0);
            painter.rect_filled(front, 0.0, bg);
            painter.rect_stroke(front, 0.0, stroke);
        }
        WindowButton::Maximize => {
            painter.rect_stroke(square(center.x - half, center.y - half), 0.0, stroke);
        }
        WindowButton::Close => {
            let d = vec2(half, half);
            painter.line_segment([center - d, center + d], stroke);
            painter.line_segment(
                [center + vec2(half, -half), center + vec2(-half, half)],
                stroke,
            );
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
