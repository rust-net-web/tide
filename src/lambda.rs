use std::convert::TryInto;
use std::future::Future;
use std::pin::Pin;

use crate::http::Error;
use crate::{Body, Server};

impl<State> lambda_http::Handler for Server<State>
where
    State: Clone + Send + Sync + 'static,
{
    type Error = Error;
    type Response = lambda_http::Response<lambda_http::Body>;
    type Fut = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

    fn call(&self, event: lambda_http::Request, context: lambda_http::Context) -> Self::Fut {
        let server = self.clone();
        Box::pin(async move {
            let (parts, body) = event.into_parts();
            let body = match body {
                lambda_http::Body::Empty => Body::empty(),
                lambda_http::Body::Text(text) => Body::from_string(text),
                lambda_http::Body::Binary(bytes) => Body::from_bytes(bytes),
            };
            let mut req: http_types::Request = http::Request::from_parts(parts, body).try_into()?;

            req.ext_mut().insert(context);
            let res: http_types::Response = server.respond(req).await?;

            let res: http::Response<Body> = res.try_into()?;
            let (parts, body) = res.into_parts();
            let body = match body.is_empty() {
                Some(true) => lambda_http::Body::Empty,
                _ => lambda_http::Body::Binary(body.into_bytes().await?),
            };
            Ok(http::Response::from_parts(parts, body))
        })
    }
}
