//! The window frame: the title bar, the rounded corners, the border and the edges
//! used to resize. The native frame is turned off, so all of it is drawn here.

use eframe::egui::{
    self, vec2, Align2, Area, Color32, Context, CursorIcon, FontId, Frame, Id, Order, Painter,
    Pos2, Rect, ResizeDirection, Rounding, Sense, Stroke, TextureHandle, ViewportCommand,
};

use crate::theme::Colors;

/// Corner radius of the window.
const RADIUS: f32 = 10.0;
const HEIGHT: f32 = 36.0;
const BUTTON_W: f32 = 46.0;
/// Width of the strip along each edge that starts a resize.
const EDGE: f32 = 5.0;
/// Size of the square at each corner that starts a diagonal resize.
const CORNER: f32 = 12.0;
/// Size of the logo, in points.
const LOGO: f32 = 16.0;
/// Side of the logo texture, in pixels: twice its size, so it stays sharp on a 200 % screen.
const LOGO_PIXELS: u32 = 32;
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

/// Rounding of the area under the title bar: only its bottom corners are round.
pub fn body_rounding(ctx: &Context) -> Rounding {
    let r = radius(ctx);
    Rounding {
        nw: 0.0,
        ne: 0.0,
        sw: r,
        se: r,
    }
}

/// Builds the logo texture from the application icon.
pub fn load_logo(ctx: &Context) -> Option<TextureHandle> {
    let icon = image::load_from_memory(include_bytes!("../assets/icon.png")).ok()?;
    let small = icon
        .resize_exact(
            LOGO_PIXELS,
            LOGO_PIXELS,
            image::imageops::FilterType::Lanczos3,
        )
        .to_rgba8();
    let pixels = [LOGO_PIXELS as usize; 2];
    let image = egui::ColorImage::from_rgba_unmultiplied(pixels, small.as_raw());
    Some(ctx.load_texture("window_logo", image, egui::TextureOptions::LINEAR))
}

/// Rounds a position to the middle of a physical pixel, so one point wide lines are crisp.
fn crisp(ctx: &Context, v: f32) -> f32 {
    let ppp = ctx.pixels_per_point();
    ((v * ppp).floor() + 0.5) / ppp
}

/// Title bar: logo and name on the left, window buttons on the right. Dragging it moves
/// the window and a double click maximizes it. Call before the central panel.
pub fn show(ctx: &Context, c: &Colors, logo: Option<&TextureHandle>) {
    let maximized = is_maximized(ctx);
    let r = radius(ctx);
    let top_rounding = Rounding {
        nw: r,
        ne: r,
        sw: 0.0,
        se: 0.0,
    };
    egui::TopBottomPanel::top("titlebar")
        .exact_height(HEIGHT)
        .frame(Frame::none().fill(c.surface).rounding(top_rounding))
        .show(ctx, |ui| {
            let bar = ui.max_rect();
            let painter = ui.painter().clone();

            // Added first, so the buttons placed on top of it win their clicks.
            let drag = ui.interact(bar, Id::new("titlebar_drag"), Sense::click_and_drag());
            if drag.double_clicked() {
                ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
            } else if drag.drag_started() {
                ctx.send_viewport_cmd(ViewportCommand::StartDrag);
            }

            let logo_rect = Rect::from_center_size(
                bar.left_center() + vec2(12.0 + LOGO / 2.0, 0.0),
                vec2(LOGO, LOGO),
            );
            if let Some(logo) = logo {
                painter.image(
                    logo.id(),
                    logo_rect,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            painter.text(
                logo_rect.right_center() + vec2(8.0, 0.0),
                Align2::LEFT_CENTER,
                "Painty",
                FontId::proportional(12.5),
                c.text,
            );
            let line = crisp(ctx, bar.bottom() - 1.0);
            painter.hline(bar.x_range(), line, Stroke::new(1.0_f32, c.border));

            let buttons = [
                WindowButton::Minimize,
                WindowButton::Maximize,
                WindowButton::Close,
            ];
            for (i, kind) in buttons.into_iter().enumerate() {
                let left = bar.right() - BUTTON_W * (buttons.len() - i) as f32;
                let rect = Rect::from_min_size(Pos2::new(left, bar.top()), vec2(BUTTON_W, HEIGHT));
                let response = ui.interact(rect, Id::new(("titlebar_button", i)), Sense::click());
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
                // The close button follows the corner of the window.
                let rounding = Rounding {
                    ne: if close { r } else { 0.0 },
                    ..Rounding::ZERO
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

/// One-pixel outline that gives the window its shape, since it has no native border.
pub fn border(ctx: &Context, c: &Colors) {
    if is_maximized(ctx) {
        return;
    }
    let painter = ctx.layer_painter(egui::LayerId::new(
        Order::Foreground,
        Id::new("window_border"),
    ));
    let rect = ctx.screen_rect().shrink(0.5);
    painter.rect_stroke(rect, window_rounding(ctx), Stroke::new(1.0_f32, c.border));
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
