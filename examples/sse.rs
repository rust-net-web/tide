use tide::sse;

#[async_std::main]
async fn main() -> Result<(), std::io::Error> {
    tide::log::start();
    let mut app = tide::new();
    app.with(tide::log::LogMiddleware::new());
    app.at("/sse").get(sse::endpoint(|_req, sender| async move {
        sender.send("fruit", "banana", None).await?;
        sender.send("fruit", "apple", None).await?;
        Ok(())
    }));
    app.listen("localhost:8080").await?;
    Ok(())
}
