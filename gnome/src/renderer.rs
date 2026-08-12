use cairo::Context;
use crate::settings::RingLightState;

const GLOW_PASSES: usize = 5;

pub fn draw_glow(cr: &Context, width: f64, height: f64, state: &RingLightState) {
    // Clear to fully transparent
    cr.set_operator(cairo::Operator::Source);
    cr.set_source_rgba(0.0, 0.0, 0.0, 0.0);
    let _ = cr.paint();

    if !state.enabled {
        return;
    }

    cr.set_operator(cairo::Operator::Over);

    let (r, g, b) = state.glow_color();
    let brightness = state.brightness as f64;
    let base_glow_width = state.glow_width();

    // Draw multiple passes for a rich, bright glow (no per-edge dimming)
    for pass in 0..GLOW_PASSES {
        let pass_factor = 1.0 - (pass as f64 / GLOW_PASSES as f64) * 0.5;
        let alpha = brightness * pass_factor * 0.85;
        let glow_w = base_glow_width * (1.0 + pass as f64 * 0.25);

        draw_edge_glow(cr, width, height, r, g, b, alpha, glow_w);
    }

    // Punch out a circular hole around the mouse cursor
    let hole_radius = state.hole_radius();
    if hole_radius > 0.0 {
        let mx = state.mouse_x;
        let my = state.mouse_y;
        cr.set_operator(cairo::Operator::DestOut);
        let hole = cairo::RadialGradient::new(mx, my, 0.0, mx, my, hole_radius);
        hole.add_color_stop_rgba(0.0, 0.0, 0.0, 0.0, 1.0); // fully erase at center
        hole.add_color_stop_rgba(0.6, 0.0, 0.0, 0.0, 0.8);  // still mostly erased
        hole.add_color_stop_rgba(1.0, 0.0, 0.0, 0.0, 0.0);  // no erase at edge
        let _ = cr.set_source(&hole);
        let _ = cr.paint();
    }
}

fn draw_edge_glow(
    cr: &Context,
    width: f64,
    height: f64,
    r: f64,
    g: f64,
    b: f64,
    alpha: f64,
    glow_width: f64,
) {
    // Top edge
    let gradient = cairo::LinearGradient::new(0.0, 0.0, 0.0, glow_width);
    gradient.add_color_stop_rgba(0.0, r, g, b, alpha);
    gradient.add_color_stop_rgba(1.0, r, g, b, 0.0);
    let _ = cr.set_source(&gradient);
    cr.rectangle(0.0, 0.0, width, glow_width);
    let _ = cr.fill();

    // Bottom edge
    let gradient = cairo::LinearGradient::new(0.0, height, 0.0, height - glow_width);
    gradient.add_color_stop_rgba(0.0, r, g, b, alpha);
    gradient.add_color_stop_rgba(1.0, r, g, b, 0.0);
    let _ = cr.set_source(&gradient);
    cr.rectangle(0.0, height - glow_width, width, glow_width);
    let _ = cr.fill();

    // Left edge
    let gradient = cairo::LinearGradient::new(0.0, 0.0, glow_width, 0.0);
    gradient.add_color_stop_rgba(0.0, r, g, b, alpha);
    gradient.add_color_stop_rgba(1.0, r, g, b, 0.0);
    let _ = cr.set_source(&gradient);
    cr.rectangle(0.0, 0.0, glow_width, height);
    let _ = cr.fill();

    // Right edge
    let gradient = cairo::LinearGradient::new(width, 0.0, width - glow_width, 0.0);
    gradient.add_color_stop_rgba(0.0, r, g, b, alpha);
    gradient.add_color_stop_rgba(1.0, r, g, b, 0.0);
    let _ = cr.set_source(&gradient);
    cr.rectangle(width - glow_width, 0.0, glow_width, height);
    let _ = cr.fill();
}
