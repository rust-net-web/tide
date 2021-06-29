use std::io::ErrorKind;

use tide::utils::After;
use tide::{Body, Request, Response, Result, StatusCode};

#[async_std::main]
async fn main() -> Result<()> {
    tide::log::start();
    let mut app = tide::new();
    app.with(tide::log::LogMiddleware::new());

    app.with(After(|mut res: Response| async {
        if let Some(err) = res.downcast_error::<async_std::io::Error>() {
            if let ErrorKind::NotFound = err.kind() {
                let msg = format!("Error: {:?}", err);
                res.set_status(StatusCode::NotFound);

                // NOTE: You may want to avoid sending error messages in a production server.
                res.set_body(msg);
            }
        }
        Ok(res)
    }));

    app.at("/")
        .get(|_req: Request<_>| async { Ok(Body::from_file("./does-not-exist").await?) });

    app.listen("127.0.0.1:8080").await?;

    Ok(())
}
