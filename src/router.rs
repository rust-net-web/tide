use std::collections::HashMap;
use std::sync::Arc;

use crate::{
    body::Body,
    endpoint::{BoxedEndpoint, Endpoint},
    Middleware,
};
use path_table::{PathTable, RouteMatch};

/// A core type for routing.
///
/// The `Router` type can be used to set up routes and resources, and to apply middleware.
pub struct Router<Data> {
    table: PathTable<ResourceData<Data>>,
    middleware_base: Vec<Arc<dyn Middleware<Data> + Send + Sync>>,
}

pub(crate) struct RouteResult<'a, Data> {
    pub(crate) endpoint: &'a BoxedEndpoint<Data>,
    pub(crate) params: Option<RouteMatch<'a>>,
    pub(crate) middleware: &'a [Arc<dyn Middleware<Data> + Send + Sync>],
}

fn route_match_success<'a, Data>(
    route: &'a ResourceData<Data>,
    route_match: RouteMatch<'a>,
    method: &http::Method,
) -> Option<RouteResult<'a, Data>> {
    // If it is a HTTP HEAD request then check if there is a callback in the endpoints map
    // if not then fallback to the behavior of HTTP GET else proceed as usual
    let endpoint =
        if method == http::Method::HEAD && !route.endpoints.contains_key(&http::Method::HEAD) {
            route.endpoints.get(&http::Method::GET)?
        } else {
            route.endpoints.get(method)?
        };
    let middleware = &*route.middleware;

    Some(RouteResult {
        endpoint,
        params: Some(route_match),
        middleware,
    })
}

fn route_match_failure<'a, Data>(
    endpoint: &'a BoxedEndpoint<Data>,
    middleware: &'a [Arc<dyn Middleware<Data> + Send + Sync>],
) -> RouteResult<'a, Data> {
    RouteResult {
        endpoint,
        params: None,
        middleware: &*middleware,
    }
}

impl<Data: Clone + Send + Sync + 'static> Router<Data> {
    /// Add a new resource at the given `path`, relative to this router.
    ///
    /// Routing means mapping an HTTP request to an endpoint. Here Tide applies a "table of
    /// contents" approach, which makes it easy to see the overall app structure. Endpoints are
    /// selected solely by the path and HTTP method of a request: the path determines the resource
    /// and the HTTP verb the respective endpoint of the selected resource. Example:
    ///
    /// ```rust,no_run
    /// # #![feature(async_await)]
    /// # let mut app = tide::App::new(());
    /// app.at("/").get(async || "Hello, world!");
    /// ```
    ///
    /// A path is comprised of zero or many segments, i.e. non-empty strings separated by '/'. There
    /// are two kinds of segments: concrete and wildcard. A concrete segment is used to exactly
    /// match the respective part of the path of the incoming request. A wildcard segment on the
    /// other hand extracts and parses the respective part of the path of the incoming request to
    /// pass it along to the endpoint as an argument. A wildcard segment is either defined by "{}"
    /// or by "{name}" for a so called named wildcard segment which can be extracted using
    /// `NamedSegment`. It is not possible to define wildcard segments with different names for
    /// otherwise identical paths.
    ///
    /// Wildcard definitions can be followed by an optional *wildcard modifier*. Currently, there is
    /// only one modifier: `*`, which means that the wildcard will match to the end of given path,
    /// no matter how many segments are left, even nothing. If there is a modifier for unnamed
    /// wildcard definition, `{}` may be omitted. That is, `{}*` can be written as `*`. It is an
    /// error to define two wildcard segments with different wildcard modifiers, or to write other
    /// path segment after a segment with wildcard modifier.
    ///
    /// Here are some examples omitting the HTTP verb based endpoint selection:
    ///
    /// ```rust,no_run
    /// # let mut app = tide::App::new(());
    /// app.at("/");
    /// app.at("/hello");
    /// app.at("/message/{}");
    /// app.at("add_two/{num}");
    /// app.at("static/{path}*");
    /// app.at("single_page_app/*");
    /// ```
    ///
    /// Notice that there is no fallback route matching, i.e. either a resource is a full match or
    /// not, which means that the order of adding resources has no effect.
    pub fn at<'a>(&'a mut self, path: &'a str) -> Resource<'a, Data> {
        let table = self.table.setup_table(path);
        Resource {
            table,
            middleware_base: &self.middleware_base,
        }
    }

    /// Create a new top-level router.
    pub(crate) fn new() -> Router<Data> {
        Router {
            table: PathTable::new(),
            middleware_base: Vec::new(),
        }
    }

    /// Apply `middleware` to this router.
    ///
    /// Note that the order of nesting subrouters and applying middleware matters. If there are
    /// nested subrouters *before* the method call, the given middleware will be applied *after*
    /// the subrouter middleware.
    ///
    /// ```
    /// # #![feature(futures_api, async_await)]
    /// # fn passthrough_middleware<Data: Clone + Send>(
    /// #     ctx: tide::middleware::RequestContext<Data>,
    /// # ) -> futures::future::FutureObj<tide::Response> {
    /// #     ctx.next()
    /// # }
    /// # let mut app = tide::App::new(());
    /// # let router = app.router();
    /// router.at("a1").nest(|router| {
    /// #   let a = passthrough_middleware;
    ///     router.middleware(a);
    ///     router.at("").get(async || "A then B");
    /// });
    /// # let b = passthrough_middleware;
    /// router.middleware(b);
    /// router.at("a2").nest(|router| {
    /// #   let a = passthrough_middleware;
    ///     router.middleware(a);
    ///     router.at("").get(async || "B then A");
    /// });
    /// ```
    pub fn middleware(&mut self, middleware: impl Middleware<Data> + 'static) -> &mut Self {
        let middleware = Arc::new(middleware);
        for resource in self.table.iter_mut() {
            resource.middleware.push(middleware.clone());
        }
        self.middleware_base.push(middleware);
        self
    }

    pub(crate) fn route<'a>(
        &'a self,
        path: &'a str,
        method: &http::Method,
        default_handler: &'a Arc<BoxedEndpoint<Data>>,
    ) -> RouteResult<'a, Data> {
        match self.table.route(path) {
            Some((route, route_match)) => route_match_success(route, route_match, method)
                .unwrap_or_else(|| route_match_failure(default_handler, &self.middleware_base)),
            None => route_match_failure(default_handler, &self.middleware_base),
        }
    }
}

/// A handle to a resource (identified by a path).
///
/// All HTTP requests are made against resources. After using `Router::at` (or `App::at`) to
/// establish a resource path, the `Resource` type can be used to establish endpoints for various
/// HTTP methods at that path. Also, using `nest`, it can be used to set up a subrouter.
pub struct Resource<'a, Data> {
    table: &'a mut PathTable<ResourceData<Data>>,
    middleware_base: &'a Vec<Arc<dyn Middleware<Data> + Send + Sync>>,
}

struct ResourceData<Data> {
    endpoints: HashMap<http::Method, BoxedEndpoint<Data>>,
    middleware: Vec<Arc<dyn Middleware<Data> + Send + Sync>>,
}

impl<'a, Data: Send + Sync + Clone + 'static> Resource<'a, Data> {
    /// "Nest" a subrouter to the path.
    ///
    /// This method will build a fresh `Router` and give a mutable reference to it to the builder
    /// function. Builder can set up a subrouter using the `Router`. All middleware applied inside
    /// the builder will be local to the subrouter and its descendents.
    ///
    /// If resources are already present, they will be discarded.
    pub fn nest(self, builder: impl FnOnce(&mut Router<Data>)) {
        let mut subrouter = Router {
            table: PathTable::new(),
            middleware_base: self.middleware_base.clone(),
        };
        builder(&mut subrouter);
        *self.table = subrouter.table;
    }

    /// Add an endpoint for the given HTTP method
    pub fn method<T: Endpoint<Data, U>, U>(&mut self, method: http::Method, ep: T) {
        let resource = self.table.resource_mut();
        if resource.is_none() {
            let new_resource = ResourceData {
                endpoints: HashMap::new(),
                middleware: self.middleware_base.clone(),
            };
            *resource = Some(new_resource);
        }
        let resource = resource.as_mut().unwrap();
        if resource.endpoints.contains_key(&method) {
            panic!("A {} endpoint already exists for this path", method)
        }

        if !resource.endpoints.contains_key(&http::Method::OPTIONS) {
            let callback = async move || {
                http::Response::builder()
                    .status(http::status::StatusCode::OK)
                    .header("Content-Type", "text/plain")
                    .body(Body::empty())
                    .unwrap()
            };
            resource
                .endpoints
                .insert(http::Method::OPTIONS, BoxedEndpoint::new(callback));
        }
        resource.endpoints.insert(method, BoxedEndpoint::new(ep));
    }

    /// Add an endpoint for `GET` requests
    pub fn get<T: Endpoint<Data, U>, U>(&mut self, ep: T) {
        self.method(http::Method::GET, ep)
    }

    /// Add an endpoint for `HEAD` requests
    pub fn head<T: Endpoint<Data, U>, U>(&mut self, ep: T) {
        self.method(http::Method::HEAD, ep)
    }

    /// Add an endpoint for `PUT` requests
    pub fn put<T: Endpoint<Data, U>, U>(&mut self, ep: T) {
        self.method(http::Method::PUT, ep)
    }

    /// Add an endpoint for `POST` requests
    pub fn post<T: Endpoint<Data, U>, U>(&mut self, ep: T) {
        self.method(http::Method::POST, ep)
    }

    /// Add an endpoint for `DELETE` requests
    pub fn delete<T: Endpoint<Data, U>, U>(&mut self, ep: T) {
        self.method(http::Method::DELETE, ep)
    }

    /// Add an endpoint for `OPTIONS` requests
    pub fn options<T: Endpoint<Data, U>, U>(&mut self, ep: T) {
        self.method(http::Method::OPTIONS, ep)
    }

    /// Add an endpoint for `CONNECT` requests
    pub fn connect<T: Endpoint<Data, U>, U>(&mut self, ep: T) {
        self.method(http::Method::CONNECT, ep)
    }

    /// Add an endpoint for `PATCH` requests
    pub fn patch<T: Endpoint<Data, U>, U>(&mut self, ep: T) {
        self.method(http::Method::PATCH, ep)
    }

    /// Add an endpoint for `TRACE` requests
    pub fn trace<T: Endpoint<Data, U>, U>(&mut self, ep: T) {
        self.method(http::Method::TRACE, ep)
    }
}

#[cfg(test)]
mod tests {
    use futures::{executor::block_on, future::FutureObj};

    use super::*;
    use crate::{body::Body, middleware::RequestContext, AppData, Response};

    fn passthrough_middleware<Data: Clone + Send>(
        ctx: RequestContext<Data>,
    ) -> FutureObj<Response> {
        ctx.next()
    }

    async fn simulate_request<'a, Data: Default + Clone + Send + Sync + 'static>(
        router: &'a Router<Data>,
        path: &'a str,
        method: &'a http::Method,
    ) -> Option<Response> {
        let default_handler = Arc::new(BoxedEndpoint::new(async || {
            http::status::StatusCode::NOT_FOUND
        }));
        let RouteResult {
            endpoint,
            params,
            middleware,
        } = router.route(path, method, &default_handler);

        let data = Data::default();
        let req = http::Request::builder()
            .method(method)
            .body(Body::empty())
            .unwrap();

        let ctx = RequestContext {
            app_data: data,
            req,
            params,
            endpoint,
            next_middleware: middleware,
        };
        let res = await!(ctx.next());
        Some(res.map(Into::into))
    }

    fn route_middleware_count<Data: Clone + Send + Sync + 'static>(
        router: &Router<Data>,
        path: &str,
        method: &http::Method,
    ) -> Option<usize> {
        let default_handler = Arc::new(BoxedEndpoint::new(async || {
            http::status::StatusCode::NOT_FOUND
        }));
        let route_result = router.route(path, method, &default_handler);
        Some(route_result.middleware.len())
    }

    #[test]
    fn simple_static() {
        let mut router: Router<()> = Router::new();
        router.at("/").get(async || "/");
        router.at("/foo").get(async || "/foo");
        router.at("/foo/bar").get(async || "/foo/bar");

        for path in &["/", "/foo", "/foo/bar"] {
            let res =
                if let Some(res) = block_on(simulate_request(&router, path, &http::Method::GET)) {
                    res
                } else {
                    panic!("Routing of path `{}` failed", path);
                };
            let body =
                block_on(res.into_body().read_to_vec()).expect("Reading body should succeed");
            assert_eq!(&*body, path.as_bytes());
        }
    }

    #[test]
    fn nested_static() {
        let mut router: Router<()> = Router::new();
        router.at("/a").get(async || "/a");
        router.at("/b").nest(|router| {
            router.at("/").get(async || "/b");
            router.at("/a").get(async || "/b/a");
            router.at("/b").get(async || "/b/b");
            router.at("/c").nest(|router| {
                router.at("/a").get(async || "/b/c/a");
                router.at("/b").get(async || "/b/c/b");
            });
            router.at("/d").get(async || "/b/d");
        });
        router.at("/a/a").nest(|router| {
            router.at("/a").get(async || "/a/a/a");
            router.at("/b").get(async || "/a/a/b");
        });
        router.at("/a/b").nest(|router| {
            router.at("/").get(async || "/a/b");
        });

        for failing_path in &["/", "/a/a", "/a/b/a"] {
            if let Some(res) = block_on(simulate_request(&router, failing_path, &http::Method::GET))
            {
                if !res.status().is_client_error() {
                    panic!(
                        "Should have returned a client error when router cannot match with path {}",
                        failing_path
                    );
                }
            } else {
                panic!("Should have received a response from {}", failing_path);
            };
        }

        for path in &[
            "/a", "/a/a/a", "/a/a/b", "/a/b", "/b", "/b/a", "/b/b", "/b/c/a", "/b/c/b", "/b/d",
        ] {
            let res =
                if let Some(res) = block_on(simulate_request(&router, path, &http::Method::GET)) {
                    res
                } else {
                    panic!("Routing of path `{}` failed", path);
                };
            let body =
                block_on(res.into_body().read_to_vec()).expect("Reading body should succeed");
            assert_eq!(&*body, path.as_bytes());
        }
    }

    #[test]
    fn multiple_methods() {
        let mut router: Router<()> = Router::new();
        router
            .at("/a")
            .nest(|router| router.at("/b").get(async || "/a/b GET"));
        router.at("/a/b").post(async || "/a/b POST");

        for (path, method) in &[("/a/b", http::Method::GET), ("/a/b", http::Method::POST)] {
            let res = if let Some(res) = block_on(simulate_request(&router, path, &method)) {
                res
            } else {
                panic!("Routing of {} `{}` failed", method, path);
            };
            let body =
                block_on(res.into_body().read_to_vec()).expect("Reading body should succeed");
            assert_eq!(&*body, format!("{} {}", path, method).as_bytes());
        }
    }

    #[test]
    #[should_panic]
    fn duplicate_endpoint_fails() {
        let mut router: Router<()> = Router::new();
        router
            .at("/a")
            .nest(|router| router.at("/b").get(async || "")); // flattened into /a/b
        router.at("/a/b").get(async || "duplicate");
    }

    #[test]
    fn simple_middleware() {
        let mut router: Router<()> = Router::new();
        router.middleware(passthrough_middleware);
        router.at("/").get(async || "/");
        router.at("/b").nest(|router| {
            router.at("/").get(async || "/b");
            router.middleware(passthrough_middleware);
        });

        assert_eq!(
            route_middleware_count(&router, "/", &http::Method::GET),
            Some(1)
        );
        assert_eq!(
            route_middleware_count(&router, "/b", &http::Method::GET),
            Some(2)
        );
    }

    #[test]
    fn middleware_apply_order() {
        #[derive(Default, Clone, Debug)]
        struct Data(Vec<usize>);
        struct Pusher(usize);
        impl Middleware<Data> for Pusher {
            fn handle<'a>(&'a self, mut ctx: RequestContext<'a, Data>) -> FutureObj<'a, Response> {
                FutureObj::new(Box::new(
                    async move {
                        ctx.app_data.0.push(self.0);
                        await!(ctx.next())
                    },
                ))
            }
        }

        // The order of endpoint and middleware does not matter
        // The order of subrouter and middleware DOES matter
        let mut router: Router<Data> = Router::new();
        router.middleware(Pusher(0));
        router.at("/").get(async move |data: AppData<Data>| {
            if (data.0).0 == [0, 2] {
                http::StatusCode::OK
            } else {
                http::StatusCode::INTERNAL_SERVER_ERROR
            }
        });
        router.at("/a").nest(|router| {
            router.at("/").get(async move |data: AppData<Data>| {
                if (data.0).0 == [0, 1, 2] {
                    http::StatusCode::OK
                } else {
                    http::StatusCode::INTERNAL_SERVER_ERROR
                }
            });
            router.middleware(Pusher(1));
        });
        router.middleware(Pusher(2));
        router.at("/b").nest(|router| {
            router.at("/").get(async move |data: AppData<Data>| {
                if (data.0).0 == [0, 2, 1] {
                    http::StatusCode::OK
                } else {
                    http::StatusCode::INTERNAL_SERVER_ERROR
                }
            });
            router.middleware(Pusher(1));
        });

        for path in &["/", "/a", "/b"] {
            let res = block_on(simulate_request(&router, path, &http::Method::GET)).unwrap();
            assert_eq!(res.status(), 200);
        }
    }
}
