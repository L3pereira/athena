use std::path::Path;
use thiserror::Error;

use super::types::GatewayConfigFile;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Failed to read config file: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Failed to parse config: {0}")]
    ParseError(#[from] serde_json::Error),
    #[error("No enabled exchanges in config")]
    NoEnabledExchanges,
    #[error("Exchange not found: {0}")]
    ExchangeNotFound(String),
}

/// Load gateway configuration from a JSON file
pub fn load_config<P: AsRef<Path>>(path: P) -> Result<GatewayConfigFile, ConfigError> {
    let content = std::fs::read_to_string(path)?;
    let config: GatewayConfigFile = serde_json::from_str(&content)?;
    Ok(config)
}

/// Load configuration from a JSON string
pub fn load_config_from_str(json: &str) -> Result<GatewayConfigFile, ConfigError> {
    let config: GatewayConfigFile = serde_json::from_str(json)?;
    Ok(config)
}

/// Load the default embedded configuration
pub fn load_default_config() -> Result<GatewayConfigFile, ConfigError> {
    let default_config = include_str!("gateway_config.json");
    load_config_from_str(default_config)
}

impl GatewayConfigFile {
    /// Get only enabled exchanges
    pub fn enabled_exchanges(&self) -> Vec<&super::types::ExchangeConfig> {
        self.exchanges.iter().filter(|e| e.enabled).collect()
    }

    /// Get a specific exchange by ID
    pub fn get_exchange(&self, id: &str) -> Option<&super::types::ExchangeConfig> {
        self.exchanges.iter().find(|e| e.id == id)
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.enabled_exchanges().is_empty() {
            return Err(ConfigError::NoEnabledExchanges);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_default_config() {
        let config = load_default_config().unwrap();
        assert!(!config.exchanges.is_empty());
    }

    #[test]
    fn test_enabled_exchanges() {
        let config = load_default_config().unwrap();
        let enabled = config.enabled_exchanges();
        // At least simulator should be enabled
        assert!(enabled.iter().any(|e| e.id == "simulator"));
    }

    #[test]
    fn test_get_exchange() {
        let config = load_default_config().unwrap();
        let simulator = config.get_exchange("simulator");
        assert!(simulator.is_some());
        assert_eq!(simulator.unwrap().name, "Exchange Simulator");
    }
}
