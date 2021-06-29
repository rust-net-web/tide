use tide::http::Cookie;
use tide::{Request, Response, StatusCode};

/// Tide will use the the `Cookies`'s `Extract` implementation to build this parameter.
///
async fn retrieve_cookie(req: Request<()>) -> tide::Result<String> {
    Ok(format!("hello cookies: {:?}", req.cookie("hello").unwrap()))
}

async fn insert_cookie(_req: Request<()>) -> tide::Result {
    let mut res = Response::new(StatusCode::Ok);
    res.insert_cookie(Cookie::new("hello", "world"));
    Ok(res)
}

async fn remove_cookie(_req: Request<()>) -> tide::Result {
    let mut res = Response::new(StatusCode::Ok);
    res.remove_cookie(Cookie::named("hello"));
    Ok(res)
}

#[async_std::main]
async fn main() -> Result<(), std::io::Error> {
    tide::log::start();
    let mut app = tide::new();
    app.with(tide::log::LogMiddleware::new());

    app.at("/").get(retrieve_cookie);
    app.at("/set").get(insert_cookie);
    app.at("/remove").get(remove_cookie);
    app.listen("127.0.0.1:8080").await?;

    Ok(())
}
