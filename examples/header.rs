use async_std::task;
use tide::{Request, Response, StatusCode};

fn main() -> Result<(), std::io::Error> {
    task::block_on(async {
        let mut app = tide::new();

        app.at("/").get(|req: Request<()>| async move {
            let content_type = req
                .header(&"accept".parse().unwrap())
                .unwrap()
                .get(0)
                .unwrap()
                .as_str();

            let res = Response::new(StatusCode::Ok)
                .body_string(content_type.to_owned())
                .set_header("content-type".parse().unwrap(), "text/html");

            Ok(res)
        });

        app.listen("127.0.0.1:8080").await?;

        Ok(())
    })
}
