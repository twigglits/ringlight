// Glow rendering for COSMIC overlay surfaces.
//
// Uses iced Canvas with strip-based gradient rendering. Each edge surface
// (top, bottom, left, right) is rendered independently, with multi-pass
// glow for richness and a circular cursor-hole cutout.

use crate::app::Message;
use crate::settings::RingLightSettings;
use cosmic::iced::widget::canvas::{self, Cache, Frame, Geometry};
use cosmic::iced::{mouse, Color, Point, Rectangle, Size};

/// Which screen edge this surface covers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EdgeSide {
    Top = 0,
    Bottom = 1,
    Left = 2,
    Right = 3,
}

pub fn edge_from_index(i: usize) -> EdgeSide {
    match i {
        0 => EdgeSide::Top,
        1 => EdgeSide::Bottom,
        2 => EdgeSide::Left,
        _ => EdgeSide::Right,
    }
}

/// Holds a canvas cache per edge so geometry is only rebuilt when state changes.
pub struct GlowCache {
    pub caches: [Cache; 4],
}

impl GlowCache {
    pub fn new() -> Self {
        Self {
            caches: std::array::from_fn(|_| Cache::new()),
        }
    }

    pub fn clear_all(&self) {
        for c in &self.caches {
            c.clear();
        }
    }

    pub fn cache_for(&self, edge: EdgeSide) -> &Cache {
        &self.caches[edge as usize]
    }
}

/// Canvas program that renders the glow for a single screen edge.
pub struct GlowProgram<'a> {
    pub cache: &'a Cache,
    pub settings: &'a RingLightSettings,
    pub mouse_pos: (f32, f32),
    pub screen_size: (f32, f32),
    pub edge: EdgeSide,
}

impl<'a> canvas::Program<Message, cosmic::Theme> for GlowProgram<'a> {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &cosmic::iced::Renderer,
        _theme: &cosmic::Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let settings = self.settings;

        vec![self.cache.draw(renderer, bounds.size(), |frame| {
            if !settings.enabled {
                return;
            }

            let w = bounds.width;
            let h = bounds.height;
            let [r, g, b] = settings.glow_color();
            let brightness = settings.brightness;
            let base_glow_w = settings.glow_width();
            let hole_r = settings.hole_radius();

            // Convert mouse screen-coords to surface-local coords
            let (lmx, lmy) =
                mouse_to_local(self.edge, self.mouse_pos, self.screen_size, base_glow_w);

            // 5 passes for a rich, bright glow (matches original cairo renderer)
            let passes = 5;
            let strips = 40;
            for pass in 0..passes {
                let pass_factor = 1.0 - (pass as f32 / passes as f32) * 0.5;
                let alpha = brightness * pass_factor * 0.85;
                let glow_w = base_glow_w * (1.0 + pass as f32 * 0.25);

                draw_edge_strips(
                    frame, self.edge, w, h, strips, r, g, b, alpha, glow_w, lmx, lmy, hole_r,
                );
            }
        })]
    }
}

// ---------------------------------------------------------------------------
// Coordinate helpers
// ---------------------------------------------------------------------------

/// Convert screen-space mouse position to surface-local coordinates.
fn mouse_to_local(
    edge: EdgeSide,
    (mx, my): (f32, f32),
    (screen_w, screen_h): (f32, f32),
    glow_w: f32,
) -> (f32, f32) {
    match edge {
        EdgeSide::Top => (mx, my),
        EdgeSide::Bottom => (mx, my - (screen_h - glow_w)),
        EdgeSide::Left => (mx, my),
        EdgeSide::Right => (mx - (screen_w - glow_w), my),
    }
}

/// Distance from the bright (screen-edge) side of the surface.
fn dist_from_bright_edge(edge: EdgeSide, strip_center: f32, surface_extent: f32) -> f32 {
    match edge {
        // bright at 0, fades toward surface_extent
        EdgeSide::Top | EdgeSide::Left => strip_center,
        // bright at surface_extent, fades toward 0
        EdgeSide::Bottom | EdgeSide::Right => surface_extent - strip_center,
    }
}

// ---------------------------------------------------------------------------
// Strip rendering
// ---------------------------------------------------------------------------

fn draw_edge_strips(
    frame: &mut Frame,
    edge: EdgeSide,
    surface_w: f32,
    surface_h: f32,
    strips: usize,
    r: f32,
    g: f32,
    b: f32,
    base_alpha: f32,
    glow_width: f32,
    local_mx: f32,
    local_my: f32,
    hole_r: f32,
) {
    let horizontal = matches!(edge, EdgeSide::Top | EdgeSide::Bottom);
    let extent = if horizontal { surface_h } else { surface_w };
    let strip_size = extent / strips as f32;

    for i in 0..strips {
        let center = i as f32 * strip_size + strip_size * 0.5;
        let dist = dist_from_bright_edge(edge, center, extent);
        let t = dist / glow_width;
        if t > 1.0 {
            continue;
        }
        let alpha = base_alpha * (1.0 - t).powi(2);
        if alpha < 0.005 {
            continue;
        }

        let color = Color::from_rgba(r, g, b, alpha);
        let pos = i as f32 * strip_size;

        if horizontal {
            draw_h_strip(frame, pos, strip_size, surface_w, color, local_mx, local_my, hole_r);
        } else {
            draw_v_strip(frame, pos, strip_size, surface_h, color, local_mx, local_my, hole_r);
        }
    }
}

/// Draw one horizontal strip, splitting around the cursor hole if needed.
fn draw_h_strip(
    frame: &mut Frame,
    y: f32,
    h: f32,
    w: f32,
    color: Color,
    mx: f32,
    my: f32,
    hole_r: f32,
) {
    if hole_r > 0.0 {
        let dy = ((y + h * 0.5) - my).abs();
        if dy < hole_r {
            let half_chord = (hole_r * hole_r - dy * dy).sqrt();
            let hole_left = mx - half_chord;
            let hole_right = mx + half_chord;

            // Left of hole
            if hole_left > 0.0 {
                frame.fill_rectangle(
                    Point::new(0.0, y),
                    Size::new(hole_left.min(w), h + 0.5),
                    color,
                );
            }
            // Right of hole
            if hole_right < w {
                frame.fill_rectangle(
                    Point::new(hole_right.max(0.0), y),
                    Size::new((w - hole_right).max(0.0), h + 0.5),
                    color,
                );
            }
            return;
        }
    }

    frame.fill_rectangle(Point::new(0.0, y), Size::new(w, h + 0.5), color);
}

/// Draw one vertical strip, splitting around the cursor hole if needed.
fn draw_v_strip(
    frame: &mut Frame,
    x: f32,
    w: f32,
    h: f32,
    color: Color,
    mx: f32,
    my: f32,
    hole_r: f32,
) {
    if hole_r > 0.0 {
        let dx = ((x + w * 0.5) - mx).abs();
        if dx < hole_r {
            let half_chord = (hole_r * hole_r - dx * dx).sqrt();
            let hole_top = my - half_chord;
            let hole_bottom = my + half_chord;

            // Above hole
            if hole_top > 0.0 {
                frame.fill_rectangle(
                    Point::new(x, 0.0),
                    Size::new(w + 0.5, hole_top.min(h)),
                    color,
                );
            }
            // Below hole
            if hole_bottom < h {
                frame.fill_rectangle(
                    Point::new(x, hole_bottom.max(0.0)),
                    Size::new(w + 0.5, (h - hole_bottom).max(0.0)),
                    color,
                );
            }
            return;
        }
    }

    frame.fill_rectangle(Point::new(x, 0.0), Size::new(w + 0.5, h), color);
}
