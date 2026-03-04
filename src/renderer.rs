use cairo::Context;
use crate::settings::RingLightState;

const GLOW_WIDTH: f64 = 100.0;
const GLOW_PASSES: usize = 3;

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

    // Draw multiple passes for softer glow
    for pass in 0..GLOW_PASSES {
        let pass_factor = 1.0 - (pass as f64 / GLOW_PASSES as f64) * 0.6;
        let alpha = brightness * pass_factor * 0.5;
        let glow_w = GLOW_WIDTH * (1.0 + pass as f64 * 0.3);

        draw_edge_glow(cr, width, height, r, g, b, alpha, glow_w);
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
    cr.set_source(&gradient).unwrap();
    cr.rectangle(0.0, 0.0, width, glow_width);
    let _ = cr.fill();

    // Bottom edge
    let gradient = cairo::LinearGradient::new(0.0, height, 0.0, height - glow_width);
    gradient.add_color_stop_rgba(0.0, r, g, b, alpha);
    gradient.add_color_stop_rgba(1.0, r, g, b, 0.0);
    cr.set_source(&gradient).unwrap();
    cr.rectangle(0.0, height - glow_width, width, glow_width);
    let _ = cr.fill();

    // Left edge
    let gradient = cairo::LinearGradient::new(0.0, 0.0, glow_width, 0.0);
    gradient.add_color_stop_rgba(0.0, r, g, b, alpha);
    gradient.add_color_stop_rgba(1.0, r, g, b, 0.0);
    cr.set_source(&gradient).unwrap();
    cr.rectangle(0.0, 0.0, glow_width, height);
    let _ = cr.fill();

    // Right edge
    let gradient = cairo::LinearGradient::new(width, 0.0, width - glow_width, 0.0);
    gradient.add_color_stop_rgba(0.0, r, g, b, alpha);
    gradient.add_color_stop_rgba(1.0, r, g, b, 0.0);
    cr.set_source(&gradient).unwrap();
    cr.rectangle(width - glow_width, 0.0, glow_width, height);
    let _ = cr.fill();
}
