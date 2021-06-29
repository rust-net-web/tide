//! Types that represent HTTP transports and binding

mod concurrent_listener;
mod failover_listener;
#[cfg(feature = "h1-server")]
mod parsed_listener;
#[cfg(feature = "h1-server")]
mod tcp_listener;
mod to_listener;
#[cfg(feature = "h1-server")]
mod to_listener_impls;
#[cfg(all(unix, feature = "h1-server"))]
mod unix_listener;

use std::fmt::{Debug, Display};

use async_std::io;
use async_trait::async_trait;

use crate::{CancelationToken, Server};

pub use concurrent_listener::ConcurrentListener;
pub use failover_listener::FailoverListener;
pub use to_listener::ToListener;

#[cfg(feature = "h1-server")]
pub(crate) use parsed_listener::ParsedListener;
#[cfg(feature = "h1-server")]
pub(crate) use tcp_listener::TcpListener;
#[cfg(all(unix, feature = "h1-server"))]
pub(crate) use unix_listener::UnixListener;

/// The Listener trait represents an implementation of http transport for a tide
/// application. In order to provide a Listener to tide, you will also need to
/// implement at least one [`ToListener`](crate::listener::ToListener) that
/// outputs your Listener type.
#[async_trait]
pub trait Listener<State>: Debug + Display + Send + Sync + 'static
where
    State: Send + Sync + 'static,
{
    /// Bind the listener. This starts the listening process by opening the
    /// necessary network ports, but not yet accepting incoming connections. This
    /// method must be called before `accept`.
    async fn bind(&mut self, app: Server<State>) -> io::Result<()>;

    /// Start accepting incoming connections. This method must be called only
    /// after `bind` has succeeded.
    async fn accept(&mut self, cancelation_token: CancelationToken) -> io::Result<()>;

    /// Expose information about the connection. This should always return valid
    /// data after `bind` has succeeded.
    fn info(&self) -> Vec<ListenInfo>;
}

#[async_trait]
impl<L, State> Listener<State> for Box<L>
where
    L: Listener<State>,
    State: Send + Sync + 'static,
{
    async fn bind(&mut self, app: Server<State>) -> io::Result<()> {
        self.as_mut().bind(app).await
    }

    async fn accept(&mut self, cancelation_token: CancelationToken) -> io::Result<()> {
        self.as_mut().accept(cancelation_token).await
    }

    fn info(&self) -> Vec<ListenInfo> {
        self.as_ref().info()
    }
}

/// crate-internal shared logic used by tcp and unix listeners to
/// determine if an io::Error needs a backoff delay. Transient error
/// types do not require a delay.
#[cfg(feature = "h1-server")]
pub(crate) fn is_transient_error(e: &io::Error) -> bool {
    use io::ErrorKind::*;

    matches!(
        e.kind(),
        ConnectionRefused | ConnectionAborted | ConnectionReset
    )
}

/// Information about the `Listener`.
///
/// See [`Report`](../listener/trait.Report.html) for more.
#[derive(Debug, Clone)]
pub struct ListenInfo {
    conn_string: String,
    transport: String,
    tls: bool,
}

impl ListenInfo {
    /// Create a new instance of `ListenInfo`.
    ///
    /// This method should only be called when implementing a new Tide `listener`
    /// strategy.
    pub fn new(conn_string: String, transport: String, tls: bool) -> Self {
        Self {
            conn_string,
            transport,
            tls,
        }
    }

    /// Get the connection string.
    pub fn connection(&self) -> &str {
        self.conn_string.as_str()
    }

    /// The underlying transport this connection listens on.
    ///
    /// Examples are: "tcp", "uds", etc.
    pub fn transport(&self) -> &str {
        self.transport.as_str()
    }

    /// Is the connection encrypted?
    pub fn is_encrypted(&self) -> bool {
        self.tls
    }
}

impl Display for ListenInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.conn_string)
    }
}
