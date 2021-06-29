#[async_std::main]
async fn main() -> Result<(), std::io::Error> {
    tide::log::start();
    let mut app = tide::new();
    app.with(tide::log::LogMiddleware::new());
    app.at("/").get(|_| async { Ok("Root") });
    app.at("/api").nest({
        let mut api = tide::new();
        api.at("/hello").get(|_| async { Ok("Hello, world") });
        api.at("/goodbye").get(|_| async { Ok("Goodbye, world") });
        api
    });
    app.listen("127.0.0.1:8080").await?;
    Ok(())
}
