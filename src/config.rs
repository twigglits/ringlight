//! Settings persistence via cosmic-config.
//!
//! Every failure degrades to in-memory defaults: losing saved brightness is
//! never a reason to take down a panel applet.

use crate::settings::RingLightSettings;
use cosmic::cosmic_config::{Config, CosmicConfigEntry};

const APP_ID: &str = "com.github.twigglits.ringlight";

fn config() -> Option<Config> {
    match Config::new(APP_ID, RingLightSettings::VERSION) {
        Ok(c) => Some(c),
        Err(e) => {
            log::warn!("ringlight: cosmic-config unavailable: {e}");
            None
        }
    }
}

/// Load persisted settings, falling back to defaults.
pub fn load() -> RingLightSettings {
    let Some(cfg) = config() else {
        return RingLightSettings::default();
    };
    match RingLightSettings::get_entry(&cfg) {
        Ok(s) => s,
        Err((errors, partial)) => {
            for e in errors {
                log::warn!("ringlight: config key unreadable, using default: {e}");
            }
            partial
        }
    }
}

/// Persist settings. Errors are logged, never propagated.
pub fn save(settings: &RingLightSettings) {
    let Some(cfg) = config() else { return };
    if let Err(e) = settings.write_entry(&cfg) {
        log::warn!("ringlight: could not save settings: {e}");
    }
}

#[cfg(test)]
mod tests {
    use crate::settings::{GlowSize, HoleSize, RingLightSettings};

    #[test]
    fn settings_round_trip_through_json() {
        let original = RingLightSettings {
            enabled: true,
            brightness: 0.42,
            color_temp: 0.9,
            auto_mode: false,
            glow_size: GlowSize::Large,
            hole_size: HoleSize::Off,
        };

        let encoded = serde_json::to_string(&original).expect("serialize");
        let decoded: RingLightSettings = serde_json::from_str(&encoded).expect("deserialize");

        assert_eq!(decoded.enabled, original.enabled);
        assert_eq!(decoded.brightness, original.brightness);
        assert_eq!(decoded.color_temp, original.color_temp);
        assert_eq!(decoded.auto_mode, original.auto_mode);
        assert_eq!(decoded.glow_size, original.glow_size);
        assert_eq!(decoded.hole_size, original.hole_size);
    }

    /// Exercises the derive-generated write path against real cosmic-config
    /// storage, which the applet's own save path cannot be triggered from a
    /// test. Uses a throwaway APP_ID so it cannot clobber real settings.
    #[test]
    fn write_entry_round_trips_through_cosmic_config() {
        use cosmic::cosmic_config::{Config, CosmicConfigEntry};

        const TEST_ID: &str = "com.github.twigglits.ringlight.selftest";

        let Ok(cfg) = Config::new(TEST_ID, RingLightSettings::VERSION) else {
            // No XDG config dir (e.g. sandboxed CI); nothing to assert.
            return;
        };

        let original = RingLightSettings {
            enabled: true,
            brightness: 0.33,
            color_temp: 0.77,
            auto_mode: false,
            glow_size: GlowSize::Large,
            hole_size: HoleSize::Small,
        };

        original.write_entry(&cfg).expect("write_entry");

        let loaded = match RingLightSettings::get_entry(&cfg) {
            Ok(s) => s,
            Err((errors, _)) => panic!("get_entry failed: {errors:?}"),
        };
        assert_eq!(loaded, original);

        if let Some(dir) = dirs::config_dir() {
            let _ = std::fs::remove_dir_all(dir.join("cosmic").join(TEST_ID));
        }
    }
}
