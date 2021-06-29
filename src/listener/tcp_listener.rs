use super::{is_transient_error, ListenInfo};

use crate::listener::Listener;
use crate::{log, Server};

use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

use std::net::ToSocketAddrs;

use async_std::net::{self, SocketAddr, TcpStream};
use async_std::prelude::*;
use async_std::{io, task};
use http_types::Url;

/// This represents a tide [Listener](crate::listener::Listener) that
/// wraps an [async_std::net::TcpListener]. It is implemented as an
/// enum in order to allow creation of a tide::listener::TcpListener
/// from a SocketAddr spec that has not yet been bound OR from a bound
/// TcpListener.
///
/// ```rust,no_run
/// # async_std::task::block_on(async {
/// tide::new().listen(
///     TcpListener::new("localhost:8080")
///         .with_nodelay(true)
///         .with_ttl(100)
/// ).await
/// # });
/// ```
pub struct TcpListener<State> {
    addrs: Option<Vec<SocketAddr>>,
    listener: Option<net::TcpListener>,
    server: Option<Server<State>>,
    info: Option<ListenInfo>,
    tcp_nodelay: Option<bool>,
    tcp_ttl: Option<u32>,
}

impl<S> Default for TcpListener<S> {
    fn default() -> Self {
        TcpListener {
            addrs: None,
            listener: None,
            server: None,
            info: None,
            tcp_nodelay: None,
            tcp_ttl: None,
        }
    }
}

impl<State> TcpListener<State> {
    pub fn new(s: &str) -> crate::Result<Self> {
        Self::from_str(s)
    }

    pub fn from_addrs(addrs: impl std::net::ToSocketAddrs) -> crate::Result<Self> {
        Ok(Self {
            addrs: Some(addrs.to_socket_addrs()?.collect()),
            ..Default::default()
        })
    }

    pub fn from_listener(tcp_listener: impl Into<net::TcpListener>) -> Self {
        Self {
            listener: Some(tcp_listener.into()),
            ..Default::default()
        }
    }

    pub fn set_nodelay(&mut self, nodelay: bool) {
        self.tcp_nodelay = Some(nodelay);
    }

    pub fn nodelay(&self) -> Option<bool> {
        self.tcp_nodelay
    }

    pub fn with_nodelay(mut self, nodelay: bool) -> Self {
        self.set_nodelay(nodelay);
        self
    }

    pub fn set_ttl(&mut self, ttl: u32) {
        self.tcp_ttl = Some(ttl);
    }

    pub fn ttl(&self) -> Option<u32> {
        self.tcp_ttl
    }

    pub fn with_ttl(mut self, ttl: u32) -> Self {
        self.set_ttl(ttl);
        self
    }
}

fn handle_tcp<State: Clone + Send + Sync + 'static>(app: Server<State>, stream: TcpStream) {
    task::spawn(async move {
        let local_addr = stream.local_addr().ok();
        let peer_addr = stream.peer_addr().ok();

        let fut = async_h1::accept(stream, |mut req| async {
            req.set_local_addr(local_addr);
            req.set_peer_addr(peer_addr);
            app.respond(req).await
        });

        if let Err(error) = fut.await {
            log::error!("async-h1 error", { error: error.to_string() });
        }
    });
}

impl<State> FromStr for TcpListener<State> {
    type Err = crate::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Ok(addrs) = s.to_socket_addrs() {
            Self::from_addrs(addrs.collect::<Vec<_>>().as_slice())
        } else {
            let url = Url::parse(s)?;
            if url.scheme() == "http" {
                Self::from_addrs(url.socket_addrs(|| None)?.as_slice())
            } else {
                Err(crate::http::format_err!(
                    "tcp listener must be used with a http scheme"
                ))
            }
        }
    }
}

#[async_trait::async_trait]
impl<State> Listener<State> for TcpListener<State>
where
    State: Clone + Send + Sync + 'static,
{
    async fn bind(&mut self, server: Server<State>) -> io::Result<()> {
        assert!(self.server.is_none(), "`bind` should only be called once");
        self.server = Some(server);

        if self.listener.is_none() {
            let addrs = self
                .addrs
                .take()
                .expect("`bind` should only be called once");
            let listener = net::TcpListener::bind(addrs.as_slice()).await?;
            self.listener = Some(listener);
        }

        // Format the listen information.
        let conn_string = format!("{}", self);
        let transport = "tcp".to_owned();
        let tls = false;
        self.info = Some(ListenInfo::new(conn_string, transport, tls));

        Ok(())
    }

    async fn accept(&mut self) -> io::Result<()> {
        let server = self
            .server
            .take()
            .expect("`Listener::bind` must be called before `Listener::accept`");
        let listener = self
            .listener
            .take()
            .expect("`Listener::bind` must be called before `Listener::accept`");

        let mut incoming = listener.incoming();

        while let Some(stream) = incoming.next().await {
            match stream {
                Err(ref e) if is_transient_error(e) => continue,
                Err(error) => {
                    let delay = std::time::Duration::from_millis(500);
                    crate::log::error!("Error: {}. Pausing for {:?}.", error, delay);
                    task::sleep(delay).await;
                    continue;
                }

                Ok(stream) => {
                    if let Some(nodelay) = self.tcp_nodelay {
                        stream.set_nodelay(nodelay)?;
                    }

                    if let Some(ttl) = self.tcp_ttl {
                        stream.set_ttl(ttl)?;
                    }

                    handle_tcp(server.clone(), stream);
                }
            };
        }
        Ok(())
    }

    fn info(&self) -> Vec<ListenInfo> {
        match &self.info {
            Some(info) => vec![info.clone()],
            None => vec![],
        }
    }
}

impl<State> fmt::Debug for TcpListener<State> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("TcpListener")
            .field(&"listener", &self.listener)
            .field(&"addrs", &self.addrs)
            .field(
                &"server",
                if self.server.is_some() {
                    &"Some(Server<State>)"
                } else {
                    &"None"
                },
            )
            .finish()
    }
}

impl<State> Display for TcpListener<State> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let http_fmt = |a| format!("http://{}", a);
        match &self.listener {
            Some(listener) => {
                let addr = listener.local_addr().expect("Could not get local addr");
                write!(f, "{}", http_fmt(&addr))
            }
            None => match &self.addrs {
                Some(addrs) => {
                    let addrs = addrs.iter().map(http_fmt).collect::<Vec<_>>().join(", ");
                    write!(f, "{}", addrs)
                }
                None => write!(f, "Not listening. Did you forget to call `Listener::bind`?"),
            },
        }
    }
}
