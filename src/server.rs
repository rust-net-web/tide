//! An HTTP server

use std::collections::BTreeMap;

use async_std::io;
use async_std::sync::Arc;
use http_types::StatusCode;
use percent_encoding::percent_decode_str;
use route_recognizer::Params;

#[cfg(feature = "cookies")]
use crate::cookies;
use crate::listener::{Listener, ToListener};
use crate::log;
use crate::middleware::{Middleware, Next};
use crate::router::{Router, Selection};
use crate::{Endpoint, Request, Route};

/// An HTTP server.
///
/// Servers are built up as a combination of *state*, *endpoints* and *middleware*:
///
/// - Server state is user-defined, and is provided via the [`Server::with_state`] function. The
/// state is available as a shared reference to all app endpoints.
///
/// - Endpoints provide the actual application-level code corresponding to
/// particular URLs. The [`Server::at`] method creates a new *route* (using
/// standard router syntax), which can then be used to register endpoints
/// for particular HTTP request types.
///
/// - Middleware extends the base Tide framework with additional request or
/// response processing, such as compression, default headers, or logging. To
/// add middleware to an app, use the [`Server::middleware`] method.
pub struct Server<State> {
    router: Arc<Router<State>>,
    state: State,
    /// Holds the middleware stack.
    ///
    /// Note(Fishrock123): We do actually want this structure.
    /// The outer Arc allows us to clone in .respond() without cloning the array.
    /// The Vec allows us to add middleware at runtime.
    /// The inner Arc-s allow MiddlewareEndpoint-s to be cloned internally.
    /// We don't use a Mutex around the Vec here because adding a middleware during execution should be an error.
    #[allow(clippy::rc_buffer)]
    middleware: Arc<Vec<Arc<dyn Middleware<State>>>>,
}

impl Server<()> {
    /// Create a new Tide server.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use async_std::task::block_on;
    /// # fn main() -> Result<(), std::io::Error> { block_on(async {
    /// #
    /// let mut app = tide::new();
    /// app.at("/").get(|_| async { Ok("Hello, world!") });
    /// app.listen("127.0.0.1:8080").await?;
    /// #
    /// # Ok(()) }) }
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self::with_state(())
    }
}

impl Default for Server<()> {
    fn default() -> Self {
        Self::new()
    }
}

impl<State> Server<State>
where
    State: Clone + Send + Sync + 'static,
{
    /// Create a new Tide server with shared application scoped state.
    ///
    /// Application scoped state is useful for storing items
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use async_std::task::block_on;
    /// # fn main() -> Result<(), std::io::Error> { block_on(async {
    /// #
    /// use tide::Request;
    ///
    /// /// The shared application state.
    /// #[derive(Clone)]
    /// struct State {
    ///     name: String,
    /// }
    ///
    /// // Define a new instance of the state.
    /// let state = State {
    ///     name: "Nori".to_string()
    /// };
    ///
    /// // Initialize the application with state.
    /// let mut app = tide::with_state(state);
    /// app.at("/").get(|req: Request<State>| async move {
    ///     Ok(format!("Hello, {}!", &req.state().name))
    /// });
    /// app.listen("127.0.0.1:8080").await?;
    /// #
    /// # Ok(()) }) }
    /// ```
    pub fn with_state(state: State) -> Self {
        Self {
            router: Arc::new(Router::new()),
            middleware: Arc::new(vec![
                #[cfg(feature = "cookies")]
                Arc::new(cookies::CookiesMiddleware::new()),
                #[cfg(feature = "logger")]
                Arc::new(log::LogMiddleware::new()),
            ]),
            state,
        }
    }

    /// Add a new route at the given `path`, relative to root.
    ///
    /// Routing means mapping an HTTP request to an endpoint. Here Tide applies
    /// a "table of contents" approach, which makes it easy to see the overall
    /// app structure. Endpoints are selected solely by the path and HTTP method
    /// of a request: the path determines the resource and the HTTP verb the
    /// respective endpoint of the selected resource. Example:
    ///
    /// ```rust,no_run
    /// # let mut app = tide::Server::new();
    /// app.at("/").get(|_| async { Ok("Hello, world!") });
    /// ```
    ///
    /// A path is comprised of zero or many segments, i.e. non-empty strings
    /// separated by '/'. There are two kinds of segments: concrete and
    /// wildcard. A concrete segment is used to exactly match the respective
    /// part of the path of the incoming request. A wildcard segment on the
    /// other hand extracts and parses the respective part of the path of the
    /// incoming request to pass it along to the endpoint as an argument. A
    /// wildcard segment is written as `:name`, which creates an endpoint
    /// parameter called `name`. It is not possible to define wildcard segments
    /// with different names for otherwise identical paths.
    ///
    /// Alternatively a wildcard definitions can start with a `*`, for example
    /// `*path`, which means that the wildcard will match to the end of given
    /// path, no matter how many segments are left, even nothing.
    ///
    /// The name of the parameter can be omitted to define a path that matches
    /// the required structure, but where the parameters are not required.
    /// `:` will match a segment, and `*` will match an entire path.
    ///
    /// Here are some examples omitting the HTTP verb based endpoint selection:
    ///
    /// ```rust,no_run
    /// # let mut app = tide::Server::new();
    /// app.at("/");
    /// app.at("/hello");
    /// app.at("add_two/:num");
    /// app.at("files/:user/*");
    /// app.at("static/*path");
    /// app.at("static/:context/:");
    /// ```
    ///
    /// There is no fallback route matching, i.e. either a resource is a full
    /// match or not, which means that the order of adding resources has no
    /// effect.
    pub fn at<'a>(&'a mut self, path: &str) -> Route<'a, State> {
        let router = Arc::get_mut(&mut self.router)
            .expect("Registering routes is not possible after the Server has started");
        Route::new(router, path.to_owned())
    }

    /// Add middleware to an application.
    ///
    /// Middleware provides customization of the request/response cycle, such as compression,
    /// logging, or header modification. Middleware is invoked when processing a request, and can
    /// either continue processing (possibly modifying the response) or immediately return a
    /// response. See the [`Middleware`] trait for details.
    ///
    /// Middleware can only be added at the "top level" of an application, and is processed in the
    /// order in which it is applied.
    pub fn with<M>(&mut self, middleware: M) -> &mut Self
    where
        M: Middleware<State>,
    {
        log::trace!("Adding middleware {}", middleware.name());
        let m = Arc::get_mut(&mut self.middleware)
            .expect("Registering middleware is not possible after the Server has started");
        m.push(Arc::new(middleware));
        self
    }

    /// Asynchronously serve the app with the supplied listener.
    ///
    /// This is a shorthand for calling `Server::bind`, logging the `ListenInfo`
    /// instances from `Listener::info`, and then calling `Listener::accept`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use async_std::task::block_on;
    /// # fn main() -> Result<(), std::io::Error> { block_on(async {
    /// #
    /// let mut app = tide::new();
    /// app.at("/").get(|_| async { Ok("Hello, world!") });
    /// app.listen("127.0.0.1:8080").await?;
    /// #
    /// # Ok(()) }) }
    /// ```
    pub async fn listen<L: ToListener<State>>(self, listener: L) -> io::Result<()> {
        let mut listener = listener.to_listener()?;
        listener.bind(self).await?;
        for info in listener.info().iter() {
            log::info!("Server listening on {}", info);
        }
        listener.accept().await?;
        Ok(())
    }

    /// Asynchronously bind the listener.
    ///
    /// Bind the listener. This starts the listening process by opening the
    /// necessary network ports, but not yet accepting incoming connections.
    /// `Listener::listen` should be called after this to start accepting
    /// connections.
    ///
    /// When calling `Listener::info` multiple `ListenInfo` instances may be
    /// returned. This is useful when using for example `ConcurrentListener`
    /// which enables a single server to listen on muliple ports.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use async_std::task::block_on;
    /// # fn main() -> Result<(), std::io::Error> { block_on(async {
    /// #
    /// use tide::prelude::*;
    ///
    /// let mut app = tide::new();
    /// app.at("/").get(|_| async { Ok("Hello, world!") });
    /// let mut listener = app.bind("127.0.0.1:8080").await?;
    /// for info in listener.info().iter() {
    ///     println!("Server listening on {}", info);
    /// }
    /// listener.accept().await?;
    /// #
    /// # Ok(()) }) }
    /// ```
    pub async fn bind<L: ToListener<State>>(
        self,
        listener: L,
    ) -> io::Result<<L as ToListener<State>>::Listener> {
        let mut listener = listener.to_listener()?;
        listener.bind(self).await?;
        Ok(listener)
    }

    /// Respond to a `Request` with a `Response`.
    ///
    /// This method is useful for testing endpoints directly,
    /// or for creating servers over custom transports.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[async_std::main]
    /// # async fn main() -> http_types::Result<()> {
    /// #
    /// use tide::http::{Url, Method, Request, Response};
    ///
    /// let mut app = tide::new();
    /// app.at("/").get(|_| async { Ok("hello world") });
    ///
    /// let req = Request::new(Method::Get, Url::parse("https://example.com")?);
    /// let res: Response = app.respond(req).await?;
    ///
    /// assert_eq!(res.status(), 200);
    /// #
    /// # Ok(()) }
    /// ```
    pub async fn respond<Req, Res>(&self, req: Req) -> http_types::Result<Res>
    where
        Req: Into<http_types::Request>,
        Res: From<http_types::Response>,
    {
        let req = req.into();
        let Self {
            router,
            state,
            middleware,
        } = self.clone();

        let method = req.method().to_owned();
        let Selection { endpoint, params } = router.route(&req.url().path(), method);
        let route_params = vec![decode_params(&params)?];
        let req = Request::new(state, req, route_params);

        let next = Next {
            endpoint,
            next_middleware: &middleware,
        };

        let res = next.run(req).await;
        let res: http_types::Response = res.into();
        Ok(res.into())
    }

    /// Gets a reference to the server's state. This is useful for testing and nesting:
    ///
    /// # Example
    ///
    /// ```rust
    /// # #[derive(Clone)] struct SomeAppState;
    /// let mut app = tide::with_state(SomeAppState);
    /// let mut admin = tide::with_state(app.state().clone());
    /// admin.at("/").get(|_| async { Ok("nested app with cloned state") });
    /// app.at("/").nest(admin);
    /// ```
    pub fn state(&self) -> &State {
        &self.state
    }
}

impl<State: Send + Sync + 'static> std::fmt::Debug for Server<State> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Server").finish()
    }
}

impl<State: Clone> Clone for Server<State> {
    fn clone(&self) -> Self {
        Self {
            router: self.router.clone(),
            state: self.state.clone(),
            middleware: self.middleware.clone(),
        }
    }
}

/// Decode all parameters from params and add the decoded parameters to a map
fn decode_params(params: &Params) -> http_types::Result<BTreeMap<String, String>> {
    let mut map = BTreeMap::new();
    for (param, value) in params.into_iter() {
        map.insert(
            param.to_string(),
            percent_decode_str(value)
                .decode_utf8()
                .map_err(|err| http_types::Error::new(StatusCode::BadRequest, err))?
                .into(),
        );
    }

    Ok(map)
}

#[async_trait::async_trait]
impl<State: Clone + Sync + Send + 'static, InnerState: Clone + Sync + Send + 'static>
    Endpoint<State> for Server<InnerState>
{
    async fn call(&self, req: Request<State>) -> crate::Result {
        let Request {
            req,
            mut route_params,
            ..
        } = req;
        let path = req.url().path().to_owned();
        let method = req.method().to_owned();
        let router = self.router.clone();
        let middleware = self.middleware.clone();
        let state = self.state.clone();

        let Selection { endpoint, params } = router.route(&path, method);
        route_params.push(decode_params(&params)?);
        let req = Request::new(state, req, route_params);

        let next = Next {
            endpoint,
            next_middleware: &middleware,
        };

        Ok(next.run(req).await)
    }
}

#[crate::utils::async_trait]
impl<State: Clone + Send + Sync + Unpin + 'static> http_client::HttpClient for Server<State> {
    async fn send(&self, req: crate::http::Request) -> crate::http::Result<crate::http::Response> {
        self.respond(req).await
    }
}

#[cfg(test)]
mod test {
    use crate as tide;

    #[test]
    fn allow_nested_server_with_same_state() {
        let inner = tide::new();
        let mut outer = tide::new();
        outer.at("/foo").get(inner);
    }

    #[test]
    fn allow_nested_server_with_different_state() {
        let inner = tide::with_state(1);
        let mut outer = tide::new();
        outer.at("/foo").get(inner);
    }
}
