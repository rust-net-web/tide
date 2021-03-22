#[async_std::main]
async fn main() -> Result<(), std::io::Error> {
    tide::log::start();
    let mut app = tide::new();
    app.at("/").get(|_| async { Ok("visit /src/*") });
    app.at("/src").serve_dir("src/")?;

    // Make sure examples/static_file.html is available relative to the current-dir this example is run from or replace this with an absolute path.
    app.at("/example").serve_file("examples/static_file.html")?;

    app.listen("127.0.0.1:8080").await?;
    Ok(())
}
