//! Content serving utilities with support for conditional requests
//! as defined in [RFC 7232](https://tools.ietf.org/html/rfc7232)

use async_std::io::{BufRead as AsyncBufRead, Read as AsyncRead, Seek as AsyncSeek};

use crate::{Request, Response, Result, StatusCode};

/// A HTTP ressource modification state.
///
/// The modification state can be verified
/// against a `Request` with conditional headers to eventually serve a
/// `304 Not modified` or `206 Partial Content` response.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ModState {
    /// The ressource has been modified.
    Modified,
    /// The ressource last modification date.
    /// Used with `If-Modified-since`, `If-Unmodified-Since`
    /// and `If-Range` conditional headers.
    ///
    /// It is considered as a strong validator for `If-Range` requests.
    Date(http_types::utils::HttpDate),
    /// A ressource's version identifier.
    /// Used with `If-None-Match`, `If-Match` and `If-Range` conditional headers.
    Etag(http_types::conditional::ETag),
}

/// Serve content according to its modification state and the request's conditional headers:
/// If-Match, If-None-Match, If-Modified-Since, If-Unmodified-Since, Range, If-Range.
///
/// The few first bytes of the content is read to guess the MIME Type.
///
/// # Examples
///
/// Serve a static file returning `304 Not Modified` if the file's modification date
/// is earlier than the request's `If-Modified-Since` header.
///
/// ```
/// # #[allow(dead_code)]
/// async fn route_handler(request: tide::Request<()>) -> tide::Result {
///     use async_std::io::BufReader;
///     use async_std::fs::File;
///     use tide::utils::{serve_content, ModState};
///     
///     let file = File::open("/foo/bar").await?;
///     let metadata = file.metadata().await?;
///     let mod_time = metadata.modified()?;
///     let content = BufReader::new(file);
///
///     serve_content(request, content, ModState::from(mod_time)).await
/// }
/// ```
pub async fn serve_content<S, T>(req: Request<S>, content: T, mod_state: ModState) -> Result
where
    T: AsyncRead + AsyncBufRead + AsyncSeek + Send + Sync + Unpin + 'static,
{
    let res = Response::new(StatusCode::Ok);
    serve_content_with(req, res, content, mod_state).await
}

/// Similar than `serve_content` but allows to use a predefined `Response`.
///
/// `serve_content_with` only modifies the response content, status and headers
/// related to conditional requests. The MIME type is not guessed from the content data.
///
/// # Examples
///
/// Serve a static file returning `304 Not Modified` if the file's modification date
/// is earlier than the request's `If-Modified-Since` header, with HTML MIME type.
///
/// ```
/// # use tide::{Response, Redirect, Request, StatusCode};
/// # use async_std::io::BufReader;
/// # #[allow(dead_code)]
/// async fn route_handler(request: Request<()>) -> tide::Result {
///     use tide::utils::{serve_content, ModState};
///     
///     let file = async_std::fs::File::open("/foo/bar").await?;
///     let metadata = file.metadata().await?;
///     let mod_time = metadata.modified()?;
///     let content = BufReader::new(file);
///
///     let response = tide::Response::builder(200).content_type(tide::mime::HTML);
///
///     serve_content_with(request, response, content, ModState::from(mod_time)).await
/// }
/// ```
pub async fn serve_content_with<S, T>(
    req: Request<S>,
    res: Response,
    content: T,
    mod_state: ModState,
) -> Result
where
    T: AsyncRead + AsyncBufRead + AsyncSeek + Send + Sync + Unpin + 'static,
{
    todo!();
}
