use serde_derive::{Serialize, Deserialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Config {
    // Identifiers sent to nsqd representing this client (consumer specific)
    pub client_id: Option<String>,
    // hostname where client is deployed.
    pub hostname: Option<String>,

    // enable feature_negotiation
    pub feature_negotiation: bool,

    // Duration of time between heartbeats (milliseconds).
    // Valid values:
    // -1 disables heartbeats
    // 1000 <= heartbeat_interval <= configured_max
    pub heartbeat_interval: i64,

    // Size of the buffer (in bytes) used by nsqd for buffering writes to this connection
    // Valid values:
    // -1 disable output buffer
    // 64 <= output_buffer_size <= configured_max
    pub output_buffer_size: u64,

    // The timeout after which data nsqd has buffered will be flushed to this client.
    // valid values:
    // -1 disable buffer timeout
    // 1ms <= output_buffer_timeout <= configured_max
    pub output_buffer_timeout: u32,

    // Enable TLS negotiation
    pub tls_v1: bool,

    // Enable snappy compression.
    pub snappy: bool,

    // Enable deflate compression.
    pub deflate: bool,
    // Configure deflate compression level.
    // Valid range:
    // 1 <= deflate_level <= configured_max
    pub deflate_level: u16,

    // Integer percentage to sample the channel.
    // Deliver a perventage of all messages received to this connection.
    pub sample_rate: u16,

    // String indentifying the agent for this connection.
    pub user_agent: String,

    // Timeout used by nsqd before flushing buffered writes (set to 0 to disable).
    pub message_timeout: u32,

}
use hostname::get_hostname;

impl Default for Config {
    fn default() -> Config {
        Config {
            client_id: get_hostname(),
            user_agent: String::from("nsqueue"),
            hostname: get_hostname(),
            deflate: false,
            deflate_level: 6,
            snappy: false,
            feature_negotiation: true,
            heartbeat_interval: 30000,
            //heartbeat_interval: 2000,
            message_timeout: 0,
            output_buffer_size: 16384,
            output_buffer_timeout: 250,
            sample_rate: 0,
            tls_v1: false,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct NsqdConfig {
    pub max_rdy_count: u32,
    pub version: String,
    pub max_msg_timeout: u64,
    pub msg_timeout: u64,
    pub tls_v1: bool,
    pub deflate: bool,
    pub deflate_level: u16,
    pub max_deflate_level: u16,
    pub snappy: bool,
    pub sample_rate: u16,
    pub auth_required: bool,
    pub output_buffer_size: u64,
    pub output_buffer_timeout: u32,
}

#[allow(dead_code)]
impl Config {
    pub fn new() -> Config {
        Config{ ..Default::default() }
    }

    pub fn client_id(mut self, client_id: String) -> Self {
        self.client_id = Some(client_id);
        self
    }

    pub fn hostname(mut self, hostname: String) -> Self {
        self.hostname = Some(hostname);
        self
    }

    pub fn user_agent(mut self, user_agent: String) -> Self {
        self.user_agent = user_agent;
        self
    }

    pub fn snappy(mut self, snappy: bool) -> Self {
        self.snappy = snappy;
        self
    }
}
