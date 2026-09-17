// Legacy defaults - now using ui/themes/presets.rs for configuration
// This file kept for backward compatibility

use super::types::Config;

impl Default for Config {
    fn default() -> Self {
        // Fresh installs start on the cometix theme with every segment enabled
        crate::ui::themes::ThemePresets::get_cometix()
    }
}
