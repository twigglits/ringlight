use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum GlowSize {
    Small,
    Medium,
    Large,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum HoleSize {
    Off,
    Small,
    Medium,
    Large,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RingLightSettings {
    pub enabled: bool,
    pub brightness: f32,
    /// 0.0 = warm amber, 1.0 = cool white
    pub color_temp: f32,
    pub auto_mode: bool,
    pub glow_size: GlowSize,
    pub hole_size: HoleSize,
}

impl Default for RingLightSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            brightness: 0.7,
            color_temp: 0.5,
            auto_mode: true,
            glow_size: GlowSize::Medium,
            hole_size: HoleSize::Medium,
        }
    }
}

impl RingLightSettings {
    /// RGB color for the current color temperature (warm amber → cool white).
    /// Returns values in 0.0..1.0 range.
    pub fn glow_color(&self) -> [f32; 3] {
        let t = self.color_temp;
        [
            (255.0 + (220.0 - 255.0) * t) / 255.0,
            (200.0 + (230.0 - 200.0) * t) / 255.0,
            (140.0 + (255.0 - 140.0) * t) / 255.0,
        ]
    }

    /// Glow depth as a fraction of the smaller screen dimension.
    ///
    /// Proportional rather than a fixed pixel count, so the glow looks the
    /// same on any display. The old fixed 180px was 18% of a 1000px logical
    /// height, which is why it read as far too thick.
    pub fn glow_fraction(&self) -> f32 {
        match self.glow_size {
            GlowSize::Small => 0.06,
            GlowSize::Medium => 0.10,
            GlowSize::Large => 0.16,
        }
    }

    /// Cursor hole radius as a fraction of the smaller screen dimension.
    pub fn hole_fraction(&self) -> f32 {
        match self.hole_size {
            HoleSize::Off => 0.0,
            HoleSize::Small => 0.08,
            HoleSize::Medium => 0.14,
            HoleSize::Large => 0.22,
        }
    }
}

/// Convert a fraction of the smaller screen dimension into pixels.
///
/// `resolution` must be in physical pixels, so the result is too.
pub fn scale_to_min_dimension(fraction: f32, resolution: [f32; 2]) -> f32 {
    fraction * resolution[0].min(resolution[1])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glow_fractions_are_ordered_and_sane() {
        let small = RingLightSettings { glow_size: GlowSize::Small, ..Default::default() };
        let medium = RingLightSettings { glow_size: GlowSize::Medium, ..Default::default() };
        let large = RingLightSettings { glow_size: GlowSize::Large, ..Default::default() };

        assert!(small.glow_fraction() < medium.glow_fraction());
        assert!(medium.glow_fraction() < large.glow_fraction());
        // A glow wider than a quarter of the screen stops being a glow.
        assert!(large.glow_fraction() < 0.25);
        assert!(small.glow_fraction() > 0.0);
    }

    #[test]
    fn hole_off_is_zero() {
        let off = RingLightSettings { hole_size: HoleSize::Off, ..Default::default() };
        assert_eq!(off.hole_fraction(), 0.0);
    }

    #[test]
    fn scaling_uses_the_smaller_dimension() {
        // Landscape: height is smaller, so it governs.
        assert_eq!(scale_to_min_dimension(0.10, [3000.0, 2000.0]), 200.0);
        // Portrait: width governs.
        assert_eq!(scale_to_min_dimension(0.10, [2000.0, 3000.0]), 200.0);
        // Square.
        assert_eq!(scale_to_min_dimension(0.5, [1000.0, 1000.0]), 500.0);
    }

    #[test]
    fn medium_glow_matches_the_spec_table() {
        let s = RingLightSettings { glow_size: GlowSize::Medium, ..Default::default() };
        // 3000x2000 physical at 200% => 200 physical px => 100 logical px.
        assert_eq!(scale_to_min_dimension(s.glow_fraction(), [3000.0, 2000.0]), 200.0);
    }

    #[test]
    fn glow_color_endpoints_are_warm_and_cool() {
        let warm = RingLightSettings { color_temp: 0.0, ..Default::default() };
        let cool = RingLightSettings { color_temp: 1.0, ..Default::default() };
        // Warm has more red than blue; cool has more blue than warm.
        assert!(warm.glow_color()[0] > warm.glow_color()[2]);
        assert!(cool.glow_color()[2] > warm.glow_color()[2]);
    }
}
