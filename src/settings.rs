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

    /// Glow width in pixels from screen edge inward.
    pub fn glow_width(&self) -> f32 {
        match self.glow_size {
            GlowSize::Small => 90.0,
            GlowSize::Medium => 180.0,
            GlowSize::Large => 300.0,
        }
    }

    /// Cursor hole radius in pixels.
    pub fn hole_radius(&self) -> f32 {
        match self.hole_size {
            HoleSize::Off => 0.0,
            HoleSize::Small => 120.0,
            HoleSize::Medium => 250.0,
            HoleSize::Large => 400.0,
        }
    }
}
