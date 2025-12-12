//! Transport Configuration
//!
//! Configuration types for creating transports from config files.

use serde::{Deserialize, Serialize};

/// Transport type selector
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportType {
    /// In-process channel (default)
    #[default]
    Channel,
    /// Aeron IPC (inter-process on same machine)
    AeronIpc,
    /// Aeron UDP (reliable UDP across machines)
    AeronUdp,
}

/// Channel-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelConfig {
    /// Buffer capacity (bounded channel size)
    #[serde(default = "default_channel_capacity")]
    pub capacity: usize,
}

fn default_channel_capacity() -> usize {
    100_000
}

impl Default for ChannelConfig {
    fn default() -> Self {
        Self {
            capacity: default_channel_capacity(),
        }
    }
}

/// Aeron-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AeronConfig {
    /// Aeron channel (e.g., "aeron:ipc" or "aeron:udp?endpoint=localhost:40123")
    pub channel: String,
    /// Stream ID
    pub stream_id: i32,
    /// Media driver directory
    #[serde(default = "default_media_driver_dir")]
    pub media_driver_dir: String,
}

fn default_media_driver_dir() -> String {
    "/dev/shm/aeron".to_string()
}

impl Default for AeronConfig {
    fn default() -> Self {
        Self {
            channel: "aeron:ipc".to_string(),
            stream_id: 1001,
            media_driver_dir: default_media_driver_dir(),
        }
    }
}

/// Root transport configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportConfig {
    /// Transport type
    #[serde(rename = "type", default)]
    pub transport_type: TransportType,

    /// Channel configuration (when type = "channel")
    #[serde(default)]
    pub channel: ChannelConfig,

    /// Aeron configuration (when type = "aeron_ipc" or "aeron_udp")
    #[serde(default)]
    pub aeron: AeronConfig,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            transport_type: TransportType::Channel,
            channel: ChannelConfig::default(),
            aeron: AeronConfig::default(),
        }
    }
}

impl TransportConfig {
    /// Create a channel transport config (in-process)
    pub fn channel(capacity: usize) -> Self {
        Self {
            transport_type: TransportType::Channel,
            channel: ChannelConfig { capacity },
            ..Default::default()
        }
    }

    /// Create an Aeron IPC transport config (inter-process on same machine)
    pub fn aeron_ipc(stream_id: i32) -> Self {
        Self {
            transport_type: TransportType::AeronIpc,
            aeron: AeronConfig {
                channel: "aeron:ipc".to_string(),
                stream_id,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// Create an Aeron UDP transport config (reliable UDP across machines)
    pub fn aeron_udp(endpoint: &str, stream_id: i32) -> Self {
        Self {
            transport_type: TransportType::AeronUdp,
            aeron: AeronConfig {
                channel: format!("aeron:udp?endpoint={}", endpoint),
                stream_id,
                ..Default::default()
            },
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = TransportConfig::default();
        assert_eq!(config.transport_type, TransportType::Channel);
        assert_eq!(config.channel.capacity, 100_000);
    }

    #[test]
    fn test_channel_config() {
        let config = TransportConfig::channel(50_000);
        assert_eq!(config.transport_type, TransportType::Channel);
        assert_eq!(config.channel.capacity, 50_000);
    }

    #[test]
    fn test_aeron_ipc_config() {
        let config = TransportConfig::aeron_ipc(1001);
        assert_eq!(config.transport_type, TransportType::AeronIpc);
        assert_eq!(config.aeron.channel, "aeron:ipc");
        assert_eq!(config.aeron.stream_id, 1001);
    }

    #[test]
    fn test_aeron_udp_config() {
        let config = TransportConfig::aeron_udp("localhost:40123", 1002);
        assert_eq!(config.transport_type, TransportType::AeronUdp);
        assert_eq!(config.aeron.channel, "aeron:udp?endpoint=localhost:40123");
        assert_eq!(config.aeron.stream_id, 1002);
    }

    #[test]
    fn test_config_serialization() {
        let config = TransportConfig::channel(10_000);
        let json = serde_json::to_string(&config).unwrap();
        let parsed: TransportConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.transport_type, TransportType::Channel);
        assert_eq!(parsed.channel.capacity, 10_000);
    }
}
