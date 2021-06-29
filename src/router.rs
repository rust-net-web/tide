use route_recognizer::{Match, Params, Router as MethodRouter};
use std::collections::HashMap;

use crate::endpoint::DynEndpoint;
use crate::{http::headers, http::Method, Request, Response, StatusCode};

/// The routing table used by `Server`
///
/// Internally, we have a separate state machine per http method; indexing
/// by the method first allows the table itself to be more efficient.
#[allow(missing_debug_implementations)]
pub(crate) struct Router<State> {
    method_map: HashMap<http_types::Method, MethodRouter<Box<DynEndpoint<State>>>>,
    all_method_router: MethodRouter<Box<DynEndpoint<State>>>,
}

/// The result of routing a URL
#[allow(missing_debug_implementations)]
pub(crate) struct Selection<'a, State> {
    pub(crate) endpoint: &'a DynEndpoint<State>,
    pub(crate) params: Params,
}

impl<State: Clone + Send + Sync + 'static> Router<State> {
    pub(crate) fn new() -> Self {
        Router {
            method_map: HashMap::default(),
            all_method_router: MethodRouter::new(),
        }
    }

    pub(crate) fn add(
        &mut self,
        path: &str,
        method: http_types::Method,
        ep: Box<DynEndpoint<State>>,
    ) {
        self.method_map
            .entry(method)
            .or_insert_with(MethodRouter::new)
            .add(path, ep)
    }

    pub(crate) fn add_all(&mut self, path: &str, ep: Box<DynEndpoint<State>>) {
        self.all_method_router.add(path, ep)
    }

    pub(crate) fn route(&self, path: &str, method: http_types::Method) -> Selection<'_, State> {
        if let Some(Match { handler, params }) = self
            .method_map
            .get(&method)
            .and_then(|r| r.recognize(path).ok())
        {
            Selection {
                endpoint: &**handler,
                params,
            }
        } else if let Ok(Match { handler, params }) = self.all_method_router.recognize(path) {
            Selection {
                endpoint: &**handler,
                params,
            }
        } else if method == http_types::Method::Head {
            // If it is a HTTP HEAD request then check if there is a callback in the endpoints map
            // if not then fallback to the behavior of HTTP GET else proceed as usual

            self.route(path, http_types::Method::Get)
        } else if self
            .method_map
            .iter()
            .filter(|(k, _)| **k != method)
            .any(|(_, r)| r.recognize(path).is_ok())
        {
            // If this `path` can be handled by a callback registered with a different HTTP method,
            // the server should return 405 Method Not Allowed.
            // Or for an OPTIONS request, it should response with a success and supported methods.
            let supported_methods = self.get_supported_methods(path).join(", ");
            let mut params = Params::new();
            params.insert(String::from(SUPPORTED_METHODS_PARAM_KEY), supported_methods);
            // TODO: How to pass a closure as the endpoint here?
            Selection {
                endpoint: if method == Method::Options {
                    &http_options_endpoint
                } else {
                    &method_not_allowed_endpoint
                },
                params: params,
            }
        } else {
            Selection {
                endpoint: &not_found_endpoint,
                params: Params::new(),
            }
        }
    }

    /// Get supported methods for a target resource path
    fn get_supported_methods<'a>(&'a self, path: &'a str) -> Vec<&str> {
        let basic_methods: &[&str]; // implicitly supported methods not registered in the map
        if !self
            .method_map
            .get(&Method::Head)
            .and_then(|r| r.recognize(path).ok())
            .is_some()
            && self
                .method_map
                .get(&Method::Get)
                .and_then(|r| r.recognize(path).ok())
                .is_some()
        {
            // If the endpoint has no handler for HEAD, but a handler for GET.
            basic_methods = &["OPTIONS", "HEAD"];
        } else {
            basic_methods = &["OPTIONS"];
        }
        let registered_methods = self
            .method_map
            .iter()
            .filter(|(_, r)| r.recognize(path).is_ok())
            .map(|(m, _)| m.as_ref());
        basic_methods
            .iter()
            .map(|&s| s)
            .chain(registered_methods)
            .collect::<Vec<&str>>()
    }
}

async fn not_found_endpoint<State: Clone + Send + Sync + 'static>(
    _req: Request<State>,
) -> crate::Result {
    Ok(Response::new(StatusCode::NotFound))
}

pub(crate) const SUPPORTED_METHODS_PARAM_KEY: &'static str = "_SUPPORTED_METHODS";

/// The endpoint that responses with HTTP status `405 Method Not Allowed`
///
/// The comma-seperated list of supported methods to be set in the HTTP header `Allow` will be
/// extracted from the request param named [`SUPPORTED_METHODS_PARAM_KEY`].
/// Ref: [Section 6.5.5 of IETC RFC 7231](https://tools.ietf.org/html/rfc7231#section-6.5.5).
async fn method_not_allowed_endpoint<State: Clone + Send + Sync + 'static>(
    req: Request<State>,
) -> crate::Result {
    let mut resp = Response::new(StatusCode::MethodNotAllowed);
    if let Some(supported_methods) = req.param(SUPPORTED_METHODS_PARAM_KEY).ok() {
        resp.insert_header(headers::ALLOW, supported_methods);
    }
    Ok(resp)
}

/// The default handler for the HTTP `OPTIONS` method, only meant for listing supported methods
///
/// The comma-separated list of allowed methods to be set in the HTTP header `Allow` will be
/// extracted from the request param named [`SUPPORTED_METHODS_PARAM_KEY`].
/// For CORS preflight requests (i.e. the HTTP header `Origin` is set), it is expected be overrided
/// by CORSMiddleware, if the latter is activated.
async fn http_options_endpoint<State: Clone + Send + Sync + 'static>(
    req: Request<State>,
) -> crate::Result {
    let mut resp = Response::new(StatusCode::NoContent);
    if let Some(supported_methods) = req.param(SUPPORTED_METHODS_PARAM_KEY).ok() {
        resp.insert_header(headers::ALLOW, supported_methods);
    }
    Ok(resp)
}

#[cfg(test)]
mod test {
    use crate::http::{self, Method, Request, StatusCode, Url};
    use crate::security::{CorsMiddleware, Origin};
    use crate::Response;
    use http_types::headers::HeaderValue;
    use std::collections::HashSet;

    #[async_std::test]
    async fn default_handler_for_http_options() {
        let mut app = crate::Server::new();
        app.at("/endpoint")
            .get(|_| async { Ok("Hello, GET.") })
            .post(|_| async { Ok("Hello, POST.") });
        app.at("/pendoint").post(|_| async { Ok("Hello, POST.") });

        let response: Response = app
            .respond(Request::new(
                Method::Options,
                Url::parse("http://example.com/endpoint").unwrap(),
            ))
            .await
            .unwrap();
        assert!(response.status().is_success());
        ensure_methods_allowed(&response, &["get", "head", "post", "options"], true);

        let response: Response = app
            .respond(Request::new(
                Method::Options,
                Url::parse("http://example.com/pendoint").unwrap(),
            ))
            .await
            .unwrap();
        assert!(response.status().is_success());
        ensure_methods_allowed(&response, &["options", "post"], true);
        ensure_methods_allowed(&response, &["head"], false);
    }

    #[async_std::test]
    async fn return_status_405_if_method_not_allowed() {
        let mut app = crate::Server::new();
        app.at("/endpoint")
            .get(|_| async { Ok("Hello, GET.") })
            .post(|_| async { Ok("Hello, POST.") });

        let response: Response = app
            .respond(Request::new(
                Method::Put,
                Url::parse("http://example.com/endpoint").unwrap(),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::MethodNotAllowed);
        ensure_methods_allowed(&response, &["get", "post", "options"], true);
    }

    #[async_std::test]
    async fn options_overrided_for_cors_preflight() {
        let mut app = crate::Server::new();
        app.at("/").get(|_| async { Ok("Hello, world.") });
        app.with(
            CorsMiddleware::new()
                .allow_methods("GET, POST, OPTIONS".parse::<HeaderValue>().unwrap())
                .allow_origin(Origin::Any),
        );

        let self_origin = "example.org";
        let mut request = Request::new(Method::Options, Url::parse("http://example.com/").unwrap());
        request.append_header(http::headers::ORIGIN, self_origin);
        let response: Response = app.respond(request).await.unwrap();
        let allowed_origin = response
            .header(http::headers::ACCESS_CONTROL_ALLOW_ORIGIN)
            .map(|origin| Origin::from(origin.as_str()));
        assert_eq!(allowed_origin.unwrap(), Origin::from(self_origin));
    }

    fn ensure_methods_allowed(response: &Response, expected_methods: &[&str], positive: bool) {
        let allowed_methods = response.header(http::headers::ALLOW).map(|methods| {
            methods
                .as_str()
                .split(",")
                .map(|method| method.trim().to_ascii_lowercase())
                .collect::<HashSet<String>>()
        });
        let allowed_methods = allowed_methods.unwrap();
        for method in expected_methods
            .iter()
            .map(|&method| method.to_ascii_lowercase())
        {
            assert!(!positive ^ allowed_methods.contains(&method));
        }
    }
}
