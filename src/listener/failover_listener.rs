use crate::listener::{Listener, ToListener};
use crate::{CancelationToken, Server};

use std::fmt::{self, Debug, Display, Formatter};

use async_std::io;

use crate::listener::ListenInfo;

/// FailoverListener allows tide to attempt to listen in a sequential
/// order to any number of ports/addresses. The first successful
/// listener is used.
///
/// # Example:
/// ```rust
/// fn main() -> Result<(), std::io::Error> {
///    async_std::task::block_on(async {
///        tide::log::start();
///        let mut app = tide::new();
///        app.at("/").get(|_| async { Ok("Hello, world!") });
///
///        let mut listener = tide::listener::FailoverListener::new();
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
pub struct FailoverListener<State> {
    listeners: Vec<Option<Box<dyn Listener<State>>>>,
    index: Option<usize>,
}

impl<State> FailoverListener<State>
where
    State: Clone + Send + Sync + 'static,
{
    /// creates a new FailoverListener
    pub fn new() -> Self {
        Self {
            listeners: vec![],
            index: None,
        }
    }

    /// Adds any [`ToListener`](crate::listener::ToListener) to this
    /// FailoverListener. An error result represents a failure to convert
    /// the [`ToListener`](crate::listener::ToListener) into a
    /// [`Listener`](crate::listener::Listener).
    ///
    /// ```rust
    /// # fn main() -> std::io::Result<()> {
    /// let mut listener = tide::listener::FailoverListener::new();
    /// listener.add("127.0.0.1:8000")?;
    /// listener.add(("localhost", 8001))?;
    /// # if cfg!(unix) {
    /// listener.add("http+unix:///var/run/tide")?;
    /// # }
    /// # std::mem::drop(tide::new().listen(listener)); // for the State generic
    /// # Ok(()) }
    /// ```
    pub fn add<L>(&mut self, listener: L) -> io::Result<()>
    where
        L: ToListener<State>,
    {
        self.listeners.push(Some(Box::new(listener.to_listener()?)));
        Ok(())
    }

    /// `FailoverListener::with_listener` allows for chained construction of a FailoverListener:
    /// ```rust,no_run
    /// # use tide::listener::FailoverListener;
    /// # fn main() -> std::io::Result<()> { async_std::task::block_on(async move {
    /// # let app = tide::new();
    /// app.listen(
    ///     FailoverListener::new()
    ///         .with_listener("127.0.0.1:8080")
    ///         .with_listener(("localhost", 8081)),
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
impl<State> Listener<State> for FailoverListener<State>
where
    State: Clone + Send + Sync + 'static,
{
    async fn bind(&mut self, app: Server<State>) -> io::Result<()> {
        for (index, listener) in self.listeners.iter_mut().enumerate() {
            let listener = listener.as_deref_mut().expect("bind called twice");
            match listener.bind(app.clone()).await {
                Ok(_) => {
                    self.index = Some(index);
                    return Ok(());
                }
                Err(e) => {
                    crate::log::info!("unable to bind", {
                        listener: listener.to_string(),
                        error: e.to_string()
                    });
                }
            }
        }

        Err(io::Error::new(
            io::ErrorKind::AddrNotAvailable,
            "unable to bind to any supplied listener spec",
        ))
    }

    async fn accept(&mut self, cancelation_token: CancelationToken) -> io::Result<()> {
        match self.index {
            Some(index) => {
                let mut listener = self.listeners[index].take().expect("accept called twice");
                listener.accept(cancelation_token).await?;
                Ok(())
            }
            None => Err(io::Error::new(
                io::ErrorKind::AddrNotAvailable,
                "unable to listen to any supplied listener spec",
            )),
        }
    }

    fn info(&self) -> Vec<ListenInfo> {
        match self.index {
            Some(index) => match self.listeners.get(index) {
                Some(Some(listener)) => listener.info(),
                _ => vec![],
            },
            None => vec![],
        }
    }
}

impl<State> Debug for FailoverListener<State> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.listeners)
    }
}

impl<State> Display for FailoverListener<State> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let string = self
            .listeners
            .iter()
            .map(|l| match l {
                Some(l) => l.to_string(),
                None => String::new(),
            })
            .collect::<Vec<_>>()
            .join(", ");

        writeln!(f, "{}", string)
    }
}
