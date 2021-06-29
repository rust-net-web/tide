use crate::listener::{ListenInfo, Listener, ToListener};
use crate::{CancelationToken, Server};

use std::fmt::{self, Debug, Display, Formatter};

use async_std::{io, task};
use futures_util::stream::{futures_unordered::FuturesUnordered, StreamExt};

/// ConcurrentListener allows tide to listen on any number of transports
/// simultaneously (such as tcp ports, unix sockets, or tls).
///
/// # Example:
/// ```rust
/// fn main() -> Result<(), std::io::Error> {
///    async_std::task::block_on(async {
///        tide::log::start();
///        let mut app = tide::new();
///        app.at("/").get(|_| async { Ok("Hello, world!") });
///
///        let mut listener = tide::listener::ConcurrentListener::new();
///        listener.add("127.0.0.1:8000")?;
///        listener.add(async_std::net::TcpListener::bind("127.0.0.1:8001").await?)?;
/// # if cfg!(unix) {
///        listener.add("http+unix://unix.socket")?;
/// # }
///    
/// # if false {
///        app.listen(listener).await?;
/// # }
///        Ok(())
///    })
///}
///```

#[derive(Default)]
pub struct ConcurrentListener<State> {
    listeners: Vec<Box<dyn Listener<State>>>,
}

impl<State: Clone + Send + Sync + 'static> ConcurrentListener<State> {
    /// creates a new ConcurrentListener
    pub fn new() -> Self {
        Self { listeners: vec![] }
    }

    /// Adds any [`ToListener`](crate::listener::ToListener) to this
    /// ConcurrentListener. An error result represents a failure to convert
    /// the [`ToListener`](crate::listener::ToListener) into a
    /// [`Listener`](crate::listener::Listener).
    ///
    /// ```rust
    /// # fn main() -> std::io::Result<()> {
    /// let mut listener = tide::listener::ConcurrentListener::new();
    /// listener.add("127.0.0.1:8000")?;
    /// listener.add(("localhost", 8001))?;
    /// listener.add(std::net::TcpListener::bind(("localhost", 8002))?)?;
    /// # std::mem::drop(tide::new().listen(listener)); // for the State generic
    /// # Ok(()) }
    /// ```
    pub fn add<L>(&mut self, listener: L) -> io::Result<()>
    where
        L: ToListener<State>,
    {
        self.listeners.push(Box::new(listener.to_listener()?));
        Ok(())
    }

    /// `ConcurrentListener::with_listener` allows for chained construction of a ConcurrentListener:
    /// ```rust,no_run
    /// # use tide::listener::ConcurrentListener;
    /// # fn main() -> std::io::Result<()> { async_std::task::block_on(async move {
    /// # let app = tide::new();
    /// app.listen(
    ///     ConcurrentListener::new()
    ///         .with_listener("127.0.0.1:8080")
    ///         .with_listener(async_std::net::TcpListener::bind("127.0.0.1:8081").await?),
    /// ).await?;
    /// #  Ok(()) }) }
    pub fn with_listener<L>(mut self, listener: L) -> Self
    where
        L: ToListener<State>,
    {
        self.add(listener).expect("Unable to add listener");
        self
    }
}

#[async_trait::async_trait]
impl<State> Listener<State> for ConcurrentListener<State>
where
    State: Clone + Send + Sync + 'static,
{
    async fn bind(&mut self, app: Server<State>) -> io::Result<()> {
        for listener in self.listeners.iter_mut() {
            listener.bind(app.clone()).await?;
        }
        Ok(())
    }

    async fn accept(&mut self, cancelation_token: CancelationToken) -> io::Result<()> {
        let mut futures_unordered = FuturesUnordered::new();

        let mut cancelation_tokens = Vec::new();

        for listener in self.listeners.iter_mut() {
            let sub_cancelation_token = CancelationToken::new();
            futures_unordered.push(listener.accept(sub_cancelation_token.clone()));
            cancelation_tokens.push(sub_cancelation_token);
        }

        task::spawn(async move {
            cancelation_token.return_on_cancel::<()>(()).await;
            for sub_cancelation_token in cancelation_tokens.iter_mut() {
                sub_cancelation_token.complete();
            }
        });

        while let Some(result) = futures_unordered.next().await {
            result?;
        }
        Ok(())
    }

    fn info(&self) -> Vec<ListenInfo> {
        self.listeners
            .iter()
            .map(|listener| listener.info().into_iter())
            .flatten()
            .collect()
    }
}

impl<State> Debug for ConcurrentListener<State> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.listeners)
    }
}

impl<State> Display for ConcurrentListener<State> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let string = self
            .listeners
            .iter()
            .map(|l| l.to_string())
            .collect::<Vec<_>>()
            .join(", ");

        writeln!(f, "{}", string)
    }
}
