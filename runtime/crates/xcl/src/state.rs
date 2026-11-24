// Xenstride notice (GPL-2.0-only, section 2a): this file was changed relative to ivall/xcl 9aec53d on 2026-11-23; see XENSTRIDE_CHANGES.md at the runtime root.
//! Global host state (set once at host start; defaults in unit tests).

use std::sync::OnceLock;

use crate::config::{Boot, Config};

static BOOT: OnceLock<Boot> = OnceLock::new();
static DEFAULT_CONFIG: OnceLock<Config> = OnceLock::new();

pub fn set_boot(b: Boot) {
    let _ = BOOT.set(b);
}

pub fn boot() -> Option<&'static Boot> {
    BOOT.get()
}

pub fn config() -> &'static Config {
    match BOOT.get() {
        Some(b) => &b.config,
        None => DEFAULT_CONFIG.get_or_init(|| {
            let config = Config::default();
            // Unit fixtures must never select an installed physical adapter implicitly.
            #[cfg(test)]
            {
                let mut config = config;
                config.d3d12_adapter = "warp".into();
                config.raytracing = "off".into();
                config
            }
            #[cfg(not(test))]
            { config }
        }),
    }
}

/// Folder for title data (saves, PLS, temp…) – a temp folder when no boot record.
pub fn data_dir() -> std::path::PathBuf {
    match BOOT.get() {
        Some(b) => b.data_dir.clone(),
        None => std::env::temp_dir().join("xcl_unit_data"),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn unit_defaults_force_software_gpu_without_changing_product_defaults() {
        assert_eq!(super::config().d3d12_adapter, "warp");
        assert_eq!(crate::config::Config::default().d3d12_adapter, "auto");
    }
}
