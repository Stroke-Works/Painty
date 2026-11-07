//! The window frame: the floating window controls, the rounded corners and the edges
//! used to resize. The native frame is turned off, so all is drawn here.

use eframe::egui::{
    vec2, Area, Color32, Context, CursorIcon, Id, Order, Painter, Pos2, Rect, ResizeDirection,
    Rounding, Sense, Shape, Stroke, ViewportCommand,
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

/// Floating window controls in the top-right corner. The rest of that strip is the
/// canvas. Dragging the move button or the rectangle moves the window, and a double
/// click maximizes it.
pub fn show(ctx: &Context, c: &Colors) {
    let maximized = is_maximized(ctx);
    let buttons = [
        WindowButton::Move,
        WindowButton::Minimize,
        WindowButton::Maximize,
        WindowButton::Close,
    ];
    let last = buttons.len() - 1;
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
                let sense = match kind {
                    WindowButton::Move => Sense::click_and_drag(),
                    _ => Sense::click(),
                };
                let response = ui.interact(rect, Id::new(("window_button", i)), sense);
                let close = kind == WindowButton::Close;
                if kind == WindowButton::Move && response.hovered() {
                    ctx.set_cursor_icon(CursorIcon::Grab);
                }
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
                let rounding = if i == 0 {
                    Rounding {
                        nw: CONTROLS_ROUNDING,
                        sw: CONTROLS_ROUNDING,
                        ..Rounding::ZERO
                    }
                } else if i == last {
                    Rounding {
                        ne: CONTROLS_ROUNDING,
                        se: CONTROLS_ROUNDING,
                        ..Rounding::ZERO
                    }
                } else {
                    Rounding::ZERO
                };
                painter.rect_filled(rect, rounding, bg);
                draw_glyph(ctx, &painter, rect.center(), kind, maximized, fg, bg);

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

            painter.rect_stroke(bar, CONTROLS_ROUNDING, Stroke::new(1.0_f32, c.border));
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
    let stroke = Stroke::new(1.4_f32, color);
    let (x, y) = (crisp(ctx, center.x), crisp(ctx, center.y));
    let at = |dx: f32, dy: f32| Pos2::new(x + dx, y + dy);
    let half = 4.75;
    match kind {
        WindowButton::Move => {
            // Four-direction arrow with solid heads.
            let arm = 5.0;
            let head = 3.0;
            let wing = 2.2;
            let shaft = arm - head + 0.2;
            round_line(painter, at(-shaft, 0.0), at(shaft, 0.0), stroke);
            round_line(painter, at(0.0, -shaft), at(0.0, shaft), stroke);
            for (tip, a, b) in [
                (at(-arm, 0.0), at(-arm + head, -wing), at(-arm + head, wing)),
                (at(arm, 0.0), at(arm - head, -wing), at(arm - head, wing)),
                (at(0.0, -arm), at(-wing, -arm + head), at(wing, -arm + head)),
                (at(0.0, arm), at(-wing, arm - head), at(wing, arm - head)),
            ] {
                painter.add(Shape::convex_polygon(vec![tip, a, b], color, Stroke::NONE));
            }
        }
        WindowButton::Minimize => {
            round_line(painter, at(-5.0, 0.0), at(5.0, 0.0), stroke);
        }
        WindowButton::Maximize if maximized => {
            // Two overlapping rounded squares: the back one, then the front one.
            let size = 8.0;
            let offset = 2.0;
            let back = Rect::from_min_size(at(-half + offset, -half), vec2(size, size));
            painter.rect_stroke(back, 1.4, stroke);
            let front = Rect::from_min_size(at(-half, -half + offset), vec2(size, size));
            painter.rect_filled(front, 1.4, bg);
            painter.rect_stroke(front, 1.4, stroke);
        }
        WindowButton::Maximize => {
            let square = Rect::from_min_size(at(-half, -half), vec2(2.0 * half, 2.0 * half));
            painter.rect_stroke(square, 1.4, stroke);
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
