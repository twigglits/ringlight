use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, PartialEq)]
pub enum GlowSize { Small, Medium, Large }

#[derive(Clone, Debug, PartialEq)]
pub enum HoleSize { Off, Small, Medium, Large }

#[derive(Clone, Debug)]
pub enum Preset { WarmReading, CoolDaylight, Subtle, Bright }

#[derive(Clone, Debug)]
pub struct RingLightState {
    pub enabled: bool,
    pub brightness: f32,
    pub color_temp: f32, // 0.0 = warm amber, 1.0 = cool white
    pub auto_mode: bool,     // auto-enable when camera is on
    pub camera_active: bool, // current camera state
    pub mouse_x: f64,       // current mouse position
    pub mouse_y: f64,
    pub glow_size: GlowSize,
    pub hole_size: HoleSize,
}

impl Default for RingLightState {
    fn default() -> Self {
        Self {
            enabled: false,
            brightness: 0.7,
            color_temp: 0.5,
            auto_mode: true,
            camera_active: false,
            mouse_x: -1000.0,
            mouse_y: -1000.0,
            glow_size: GlowSize::Medium,
            hole_size: HoleSize::Medium,
        }
    }
}

impl RingLightState {
    /// Get the RGB color for the current color temperature
    /// Warm: (255, 200, 140), Cool: (220, 230, 255)
    pub fn glow_color(&self) -> (f64, f64, f64) {
        let t = self.color_temp as f64;
        let r = (255.0 + (220.0 - 255.0) * t) / 255.0;
        let g = (200.0 + (230.0 - 200.0) * t) / 255.0;
        let b = (140.0 + (255.0 - 140.0) * t) / 255.0;
        (r, g, b)
    }

    pub fn glow_width(&self) -> f64 {
        match self.glow_size {
            GlowSize::Small => 90.0,
            GlowSize::Medium => 180.0,
            GlowSize::Large => 300.0,
        }
    }

    pub fn hole_radius(&self) -> f64 {
        match self.hole_size {
            HoleSize::Off => 0.0,
            HoleSize::Small => 120.0,
            HoleSize::Medium => 250.0,
            HoleSize::Large => 400.0,
        }
    }
}

pub type SharedState = Arc<Mutex<RingLightState>>;

pub fn new_shared_state() -> SharedState {
    Arc::new(Mutex::new(RingLightState::default()))
}
