use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub struct RingLightState {
    pub enabled: bool,
    pub brightness: f32,
    pub color_temp: f32, // 0.0 = warm amber, 1.0 = cool white
    pub auto_mode: bool,     // auto-enable when camera is on
    pub camera_active: bool, // current camera state
}

impl Default for RingLightState {
    fn default() -> Self {
        Self {
            enabled: false,
            brightness: 0.7,
            color_temp: 0.5,
            auto_mode: true,
            camera_active: false,
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
}

pub type SharedState = Arc<Mutex<RingLightState>>;

pub fn new_shared_state() -> SharedState {
    Arc::new(Mutex::new(RingLightState::default()))
}
