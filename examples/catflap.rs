#[cfg(unix)]
#[async_std::main]
async fn main() -> Result<(), std::io::Error> {
    use std::{env, net::TcpListener, os::unix::io::FromRawFd};
    tide::log::start();
    let mut app = tide::new();
    app.with(tide::log::LogMiddleware::new());
    app.at("/").get(|_| async { Ok(CHANGE_THIS_TEXT) });

    const CHANGE_THIS_TEXT: &str = "hello world!";

    const DOCS: &str = "
To run this example:
$ cargo install catflap cargo-watch
$ catflap -- cargo watch -x \"run --example catflap\"

and then edit this file";

    if let Some(fd) = env::var("LISTEN_FD").ok().and_then(|fd| fd.parse().ok()) {
        app.listen(unsafe { TcpListener::from_raw_fd(fd) }).await?;
    } else {
        println!("{} ({})", DOCS, file!());
    }
    Ok(())
}

#[cfg(not(unix))]
fn main() {
    panic!("this example only runs on cfg(unix) systems");
}
