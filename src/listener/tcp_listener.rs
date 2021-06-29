use super::{is_transient_error, ListenInfo};

use crate::listener::Listener;
use crate::{CancelationToken, log, Server};

use std::fmt::{self, Display, Formatter};

use async_std::net::{self, SocketAddr, TcpStream};
use async_std::prelude::*;
use async_std::{io, task};

use futures_lite::future;

/// This represents a tide [Listener](crate::listener::Listener) that
/// wraps an [async_std::net::TcpListener]. It is implemented as an
/// enum in order to allow creation of a tide::listener::TcpListener
/// from a SocketAddr spec that has not yet been bound OR from a bound
/// TcpListener.
///
/// This is currently crate-visible only, and tide users are expected
/// to create these through [ToListener](crate::ToListener) conversions.
pub struct TcpListener<State> {
    addrs: Option<Vec<SocketAddr>>,
    listener: Option<net::TcpListener>,
    server: Option<Server<State>>,
    info: Option<ListenInfo>,
}

impl<State> TcpListener<State> {
    pub fn from_addrs(addrs: Vec<SocketAddr>) -> Self {
        Self {
            addrs: Some(addrs),
            listener: None,
            server: None,
            info: None,
        }
    }

    pub fn from_listener(tcp_listener: impl Into<net::TcpListener>) -> Self {
        Self {
            addrs: None,
            listener: Some(tcp_listener.into()),
            server: None,
            info: None,
        }
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

    async fn accept(&mut self, cancelation_token: CancelationToken) -> io::Result<()> {
        let server = self
            .server
            .take()
            .expect("`Listener::bind` must be called before `Listener::accept`");
        let listener = self
            .listener
            .take()
            .expect("`Listener::bind` must be called before `Listener::accept`");

        let mut incoming = listener.incoming();

        while let Some(stream) = future::race(incoming.next(), cancelation_token.return_on_cancel(None)).await {
            match stream {
                Err(ref e) if is_transient_error(e) => continue,
                Err(error) => {
                    let delay = std::time::Duration::from_millis(500);
                    crate::log::error!("Error: {}. Pausing for {:?}.", error, delay);
                    task::sleep(delay).await;
                    continue;
                }

                Ok(stream) => {
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
