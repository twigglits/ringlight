use cairo::Context;
use crate::settings::RingLightState;

const GLOW_WIDTH: f64 = 180.0;
const GLOW_PASSES: usize = 5;
// Mouse within this distance (px) from an edge starts dimming
const MOUSE_DIM_RADIUS: f64 = 250.0;

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
    let mx = state.mouse_x;
    let my = state.mouse_y;

    // Draw multiple passes for a rich, bright glow
    for pass in 0..GLOW_PASSES {
        let pass_factor = 1.0 - (pass as f64 / GLOW_PASSES as f64) * 0.5;
        let alpha = brightness * pass_factor * 0.85;
        let glow_w = GLOW_WIDTH * (1.0 + pass as f64 * 0.25);

        draw_edge_glow(cr, width, height, r, g, b, alpha, glow_w, mx, my);
    }
}

/// Compute a dimming factor (0.0 = fully dimmed, 1.0 = no dimming)
/// based on how close the mouse is to a point on an edge.
fn mouse_dim_factor(mouse_dist: f64) -> f64 {
    if mouse_dist > MOUSE_DIM_RADIUS {
        1.0
    } else {
        // Smooth quadratic ramp: close = dim, far = bright
        let t = mouse_dist / MOUSE_DIM_RADIUS;
        t * t
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
    mx: f64,
    my: f64,
) {
    // Distance from mouse to each edge
    let dist_top = my;
    let dist_bottom = height - my;
    let dist_left = mx;
    let dist_right = width - mx;

    // Top edge
    let dim = mouse_dim_factor(dist_top);
    let a = alpha * dim;
    let gradient = cairo::LinearGradient::new(0.0, 0.0, 0.0, glow_width);
    gradient.add_color_stop_rgba(0.0, r, g, b, a);
    gradient.add_color_stop_rgba(1.0, r, g, b, 0.0);
    cr.set_source(&gradient).unwrap();
    cr.rectangle(0.0, 0.0, width, glow_width);
    let _ = cr.fill();

    // Bottom edge
    let dim = mouse_dim_factor(dist_bottom);
    let a = alpha * dim;
    let gradient = cairo::LinearGradient::new(0.0, height, 0.0, height - glow_width);
    gradient.add_color_stop_rgba(0.0, r, g, b, a);
    gradient.add_color_stop_rgba(1.0, r, g, b, 0.0);
    cr.set_source(&gradient).unwrap();
    cr.rectangle(0.0, height - glow_width, width, glow_width);
    let _ = cr.fill();

    // Left edge
    let dim = mouse_dim_factor(dist_left);
    let a = alpha * dim;
    let gradient = cairo::LinearGradient::new(0.0, 0.0, glow_width, 0.0);
    gradient.add_color_stop_rgba(0.0, r, g, b, a);
    gradient.add_color_stop_rgba(1.0, r, g, b, 0.0);
    cr.set_source(&gradient).unwrap();
    cr.rectangle(0.0, 0.0, glow_width, height);
    let _ = cr.fill();

    // Right edge
    let dim = mouse_dim_factor(dist_right);
    let a = alpha * dim;
    let gradient = cairo::LinearGradient::new(width, 0.0, width - glow_width, 0.0);
    gradient.add_color_stop_rgba(0.0, r, g, b, a);
    gradient.add_color_stop_rgba(1.0, r, g, b, 0.0);
    cr.set_source(&gradient).unwrap();
    cr.rectangle(width - glow_width, 0.0, glow_width, height);
    let _ = cr.fill();
}
