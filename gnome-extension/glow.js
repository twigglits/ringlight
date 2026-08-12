// Glow painting, kept free of any GNOME Shell imports so it can be rendered to
// a PNG by tools/render-glow.js and checked without a live session.
import Cairo from 'cairo';

const GLOW_PASSES = 5;
const MAX_ALPHA = 0.85;
// Each pass is this much wider than the last, as a fraction of the base width.
const PASS_WIDEN = 0.25;

// Fractions of the smaller screen dimension, not pixels: a fixed pixel width
// is a different fraction of the screen on every display it lands on.
export const GLOW_FRACTION = {small: 0.06, medium: 0.10, large: 0.16};

/** Warm amber (255,200,140) at 0.0 to cool white (220,230,255) at 1.0. */
export function glowColor(temp) {
    const t = Math.max(0, Math.min(1, temp));
    return [
        (255 + (220 - 255) * t) / 255,
        (200 + (230 - 200) * t) / 255,
        (140 + (255 - 140) * t) / 255,
    ];
}

export function glowWidth(width, height, size) {
    return Math.min(width, height) * (GLOW_FRACTION[size] ?? GLOW_FRACTION.medium);
}

function drawEdges(cr, width, height, r, g, b, alpha, w) {
    const band = (x0, y0, x1, y1, rx, ry, rw, rh) => {
        const grad = new Cairo.LinearGradient(x0, y0, x1, y1);
        grad.addColorStopRGBA(0.0, r, g, b, alpha);
        grad.addColorStopRGBA(1.0, r, g, b, 0.0);
        cr.setSource(grad);
        cr.rectangle(rx, ry, rw, rh);
        cr.fill();
    };

    band(0, 0, 0, w, 0, 0, width, w);                            // top
    band(0, height, 0, height - w, 0, height - w, width, w);     // bottom
    band(0, 0, w, 0, 0, 0, w, height);                           // left
    band(width, 0, width - w, 0, width - w, 0, w, height);       // right
}

/**
 * Paint the glow onto `cr`, which is assumed to cover `width` x `height`.
 * `state` is {brightness, colorTemp, glowSize}.
 *
 * Nothing here depends on the pointer, or on anything else that changes while
 * the glow is up: this runs once per settings change, never per frame.
 */
export function drawGlow(cr, width, height, state) {
    cr.setOperator(Cairo.Operator.SOURCE);
    cr.setSourceRGBA(0, 0, 0, 0);
    cr.paint();
    cr.setOperator(Cairo.Operator.OVER);

    const [r, g, b] = glowColor(state.colorTemp);
    const base = glowWidth(width, height, state.glowSize);

    // Overlapping passes: each is wider and dimmer than the last, which stacks
    // into a falloff far softer than one gradient gives.
    for (let pass = 0; pass < GLOW_PASSES; pass++) {
        const passFactor = 1.0 - (pass / GLOW_PASSES) * 0.5;
        drawEdges(cr, width, height, r, g, b,
            state.brightness * passFactor * MAX_ALPHA,
            base * (1.0 + pass * PASS_WIDEN));
    }
}
