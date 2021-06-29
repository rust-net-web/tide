#[async_std::main]
async fn main() -> Result<(), std::io::Error> {
    tide::log::start();

    let mut app = tide::new();
    app.with(tide::log::LogMiddleware::new());

    app.at("/").get(|_| async { Ok("Hello, world!") });
    app.listen("127.0.0.1:8080").await?;
    Ok(())
}
