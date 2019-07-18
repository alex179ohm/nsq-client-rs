use serde::{Deserialize, Serialize};

/// Configuration sent to nsqd to properly config the [Connection](struct.Connection.html)
///
/// # Examples
///```no-run
/// use nsq_client::{Connection, Config};
///
/// fn main() {
///     let sys = System::new("consumer");
///     let config = Config::new().client_id("consumer").user_agent("node-1");
///     Supervisor::start(|_| Connection::new(
///         "test",
///         "test",
///         "0.0.0.0:4150",
///         Some(config),
///         None,
///         None,
///     ));
///     sys.run();
/// }
///```
///
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Config<S>
where
    S: Into<String> + Clone,
{
    /// Identifiers sent to nsqd representing this client (consumer specific)
    ///
    /// Default: **hostname** where connection is started
    pub client_id: Option<String>,

    /// Hostname where client is deployed.
    ///
    /// Default: **hostname** where connection is started
    pub hostname: Option<String>,

    /// Enable feature_negotiation
    ///
    /// Default: **true**
    pub feature_negotiation: bool,

    /// Duration of time between heartbeats (milliseconds).
    ///
    /// Valid values:
    /// * -1 disables heartbeats
    /// * 1000 <= heartbeat_interval <= configured_max
    ///
    /// Default: **30000**
    pub heartbeat_interval: i64,

    /// Size of the buffer (in bytes) used by nsqd for buffering writes to this connection
    ///
    /// Valid values:
    /// * -1 disable output buffer
    /// * 64 <= output_buffer_size <= configured_max
    ///
    /// Default: **16384**
    pub output_buffer_size: u64,

    /// The timeout after which data nsqd has buffered will be flushed to this client.
    ///
    /// Valid values:
    /// * -1 disable buffer timeout
    /// * 1ms <= output_buffer_timeout <= configured_max
    ///
    /// Default: **250**
    pub output_buffer_timeout: u32,

    /// Enable TLS negotiation
    ///
    /// Default: **false** (Not implemented)
    tls_v1: bool,

    /// Enable snappy compression.
    ///
    /// Default: **false** (Not implemented)
    pub snappy: bool,

    /// Enable deflate compression.
    ///
    /// Default: **false** (Not implemented)
    deflate: bool,
    /// Configure deflate compression level.
    ///
    /// Valid range:
    /// * 1 <= deflate_level <= configured_max
    ///
    /// Default: **6**
    deflate_level: u16,

    /// Integer percentage to sample the channel.
    ///
    /// Deliver a perventage of all messages received to this connection.
    ///
    /// Default: **0**
    pub sample_rate: u16,

    /// String indentifying the agent for this connection.
    ///
    /// Default: **hostname** where connection is started
    pub user_agent: String,

    /// Timeout used by nsqd before flushing buffered writes in milliseconds (set to 0 to disable).
    /// 
    /// Default: **0**
    pub message_timeout: u32,

    /// If None Server Cert verification is disasbled (don't use in production), if Some("") use
    /// webpki mozilla ca list for verification, Some("private_ca_file") add private ca cert chain
    /// for verify server cert.
    ///
    /// Default: Some("")
    //#[serde(skip)]
    //pub private_ca: String,

    #[serde(skip)]
    pub verify_server: VerifyServerCert<S>,
}
use hostname::get_hostname;

impl<S> Default for Config<S>
where
    S: Into<String> + Clone,
{
    fn default() -> Config<S> {
        Config {
            client_id: get_hostname(),
            user_agent: String::from("nsq_client"),
            hostname: get_hostname(),
            deflate: false,
            deflate_level: 6,
            snappy: false,
            feature_negotiation: true,
            //heartbeat_interval: 2000,
            heartbeat_interval: 30000,
            message_timeout: 0,
            output_buffer_size: 16384,
            output_buffer_timeout: 250,
            sample_rate: 0,
            tls_v1: false,
            verify_server: VerifyServerCert::None,
            //private_ca: String::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Default)]
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
impl<S> Config<S>
where
    S: Into<String> + Clone,
{
    /// Create default [Config](struct.Config.html)
    /// ```no-run
    /// use nsq_client::{Config};
    ///
    /// fn main() {
    ///     let config = Config::new();
    ///     assert_eq!(config, Config::default());
    /// }
    /// ```
    pub fn new() -> Config<S> {
        Config {
            ..Default::default()
        }
    }

    /// Change [client_id](struct.Config.html#structfield.client_id)
    /// ```no-run
    /// use nsq_client::Config;
    ///
    /// fn main() {
    ///     let config = Config::new().client_id("consumer");
    ///     assert_eq!(config.client_id, Some("consumer".to_owned()));
    /// }
    /// ```
    pub fn client_id(mut self, client_id: &str) -> Self {
        self.client_id = Some(client_id.to_owned());
        self
    }

    /// Change [hostname](struct.Config.html#structfield.hostname)
    /// ```no-run
    /// use nsq_client::Config;
    ///
    /// fn main() {
    ///     let config = Config::new().hostname("node-1");
    ///     assert_eq!(config.hostname, Some("node-1".to_owned()));
    /// }
    /// ```
    pub fn hostname(mut self, hostname: &str) -> Self {
        self.hostname = Some(hostname.to_owned());
        self
    }

    /// Change [user_agent](struct.Config.html#structfield.user_agent)
    /// ```no-run
    /// use nsq_client::Config;
    ///
    /// fn main() {
    ///     let config = Config::new().user_agent("consumer-1");
    ///     assert_eq!(config.user_agent, Some("consumer-1".to_owned()));
    /// }
    /// ```
    pub fn user_agent(mut self, user_agent: &str) -> Self {
        self.user_agent = user_agent.to_owned();
        self
    }

    pub fn tls(&mut self, verify_server_cert: VerifyServerCert<S>) {
        self.tls_v1 = true;
        self.verify_server = verify_server_cert;
    }
}

#[derive(PartialEq, Clone, Debug)]
pub enum VerifyServerCert<S>
where
    S: Into<String> + Clone,
{
    None,
    PrivateCA(S),
    PublicCA,
}

impl<S> Default for VerifyServerCert<S>
where
    S: Into<String> + Clone,
{
    fn default() -> Self {
        VerifyServerCert::None
    }
}

#[derive(Clone)]
pub struct ConnConfig<S>
where
    S: Into<String> + Clone,
{
    pub secret: Option<S>,
    pub channel: String,
    pub topic: String,
}

impl<S> ConnConfig<S>
where
    S: Into<String> + Clone,
{
    pub fn new(secret: Option<S>, channel: String, topic: String) -> ConnConfig<S> {
        ConnConfig {
            secret,
            channel,
            topic,
        }
    }
}
