use super::{is_transient_error, ListenInfo};

use crate::listener::Listener;
use crate::{log, Server};

use std::fmt::{self, Display, Formatter};

use async_std::os::unix::net::{self, SocketAddr, UnixStream};
use async_std::path::PathBuf;
use async_std::prelude::*;
use async_std::{io, task};

use futures_util::future::Either;

use waitgroup::{WaitGroup, Worker};

/// This represents a tide [Listener](crate::listener::Listener) that
/// wraps an [async_std::os::unix::net::UnixListener]. It is implemented as an
/// enum in order to allow creation of a tide::listener::UnixListener
/// from a [`PathBuf`] spec that has not yet been bound OR from a bound
/// [async_std::os::unix::net::UnixListener].
///
/// This is currently crate-visible only, and tide users are expected
/// to create these through [ToListener](crate::ToListener) conversions.
pub struct UnixListener<State> {
    path: Option<PathBuf>,
    listener: Option<net::UnixListener>,
    server: Option<Server<State>>,
    info: Option<ListenInfo>,
}

impl<State> UnixListener<State> {
    pub fn from_path(path: impl Into<PathBuf>) -> Self {
        Self {
            path: Some(path.into()),
            listener: None,
            server: None,
            info: None,
        }
    }

    pub fn from_listener(unix_listener: impl Into<net::UnixListener>) -> Self {
        Self {
            path: None,
            listener: Some(unix_listener.into()),
            server: None,
            info: None,
        }
    }
}

fn handle_unix<State: Clone + Send + Sync + 'static>(
    app: Server<State>,
    stream: UnixStream,
    wait_group_worker: Worker,
) {
    task::spawn(async move {
        let _wait_group_worker = wait_group_worker;

        let local_addr = unix_socket_addr_to_string(stream.local_addr());
        let peer_addr = unix_socket_addr_to_string(stream.peer_addr());

        let opts = async_h1::ServerOptions {
            stopper: app.stopper.clone(),
            ..Default::default()
        };
        let fut = async_h1::accept_with_opts(
            stream,
            |mut req| async {
                req.set_local_addr(local_addr.as_ref());
                req.set_peer_addr(peer_addr.as_ref());
                app.respond(req).await
            },
            opts,
        );

        if let Err(error) = fut.await {
            log::error!("async-h1 error", { error: error.to_string() });
        }
    });
}

#[async_trait::async_trait]
impl<State> Listener<State> for UnixListener<State>
where
    State: Clone + Send + Sync + 'static,
{
    async fn bind(&mut self, server: Server<State>) -> io::Result<()> {
        assert!(self.server.is_none(), "`bind` should only be called once");
        self.server = Some(server);

        if self.listener.is_none() {
            let path = self.path.take().expect("`bind` should only be called once");
            let listener = net::UnixListener::bind(path).await?;
            self.listener = Some(listener);
        }

        // Format the listen information.
        let conn_string = format!("{}", self);
        let transport = "uds".to_owned();
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

        let incoming = listener.incoming();
        let mut incoming = if let Some(stopper) = server.stopper.clone() {
            Either::Left(stopper.stop_stream(incoming))
        } else {
            Either::Right(incoming)
        };
        let wait_group = WaitGroup::new();

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
                    handle_unix(server.clone(), stream, wait_group.worker());
                }
            };
        }

        wait_group.wait().await;

        Ok(())
    }

    fn info(&self) -> Vec<ListenInfo> {
        match &self.info {
            Some(info) => vec![info.clone()],
            None => vec![],
        }
    }
}

impl<State> fmt::Debug for UnixListener<State> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("UnixListener")
            .field(&"listener", &self.listener)
            .field(&"path", &self.path)
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

impl<State> Display for UnixListener<State> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match &self.listener {
            Some(listener) => {
                let path = listener.local_addr().expect("Could not get local path dir");
                let pathname = path
                    .as_pathname()
                    .and_then(|p| p.canonicalize().ok())
                    .expect("Could not canonicalize path dir");
                write!(f, "http+unix://{}", pathname.display())
            }
            None => match &self.path {
                Some(path) => write!(f, "http+unix://{}", path.display()),
                None => write!(f, "Not listening. Did you forget to call `Listener::bind`?"),
            },
        }
    }
}

fn unix_socket_addr_to_string(result: io::Result<SocketAddr>) -> Option<String> {
    result
        .ok()
        .as_ref()
        .and_then(SocketAddr::as_pathname)
        .and_then(|p| p.canonicalize().ok())
        .map(|pathname| format!("http+unix://{}", pathname.display()))
}
