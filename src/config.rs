//! DATUM module configuration.
//!
//! Loaded from config.toml in module data dir. Node overrides via [modules.datum] and
//! MODULE_CONFIG_* env vars.

use blvm_sdk_macros::config;
use serde::{Deserialize, Serialize};

/// DATUM pool configuration.
///
/// Config file: `config.toml` in module data dir.
/// Node override: `[modules.datum]` or `[modules.blvm-datum]` in node config.
/// Env override: `MODULE_CONFIG_POOL_URL`, etc.
#[config(name = "datum")]
#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct DatumConfig {
    /// DATUM pool URL (e.g. https://ocean.xyz/datum)
    #[serde(default)]
    #[config_env]
    pub pool_url: Option<String>,

    /// Pool username
    #[serde(default)]
    #[config_env]
    pub pool_username: Option<String>,

    /// Pool password
    #[serde(default)]
    #[config_env]
    pub pool_password: Option<String>,

    /// Pool public key (hex-encoded 32 bytes)
    #[serde(default)]
    #[config_env]
    pub pool_public_key: Option<String>,

    /// Seconds between reconnect attempts.
    #[serde(default = "default_reconnect_interval")]
    pub reconnect_interval: u64,
    /// Min difficulty for pool.
    #[serde(default)]
    pub min_difficulty: Option<u64>,
}

fn default_reconnect_interval() -> u64 {
    30
}

impl DatumConfig {
    /// Convert to ModuleContext config map for server compatibility.
    pub fn to_context_map(&self) -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        if let Some(ref url) = self.pool_url {
            m.insert("pool_url".to_string(), url.clone());
        }
        if let Some(ref u) = self.pool_username {
            m.insert("pool_username".to_string(), u.clone());
        }
        if let Some(ref p) = self.pool_password {
            m.insert("pool_password".to_string(), p.clone());
        }
        if let Some(ref pk) = self.pool_public_key {
            m.insert("pool_public_key".to_string(), pk.clone());
        }
        m
    }
}

blvm_sdk::impl_module_config!(DatumConfig);
