use async_std::prelude::*;
use std::time::Duration;

mod test_utils;
use test_utils::ServerTestingExt;

#[async_std::test]
async fn log_tests() -> tide::Result<()> {
    let mut logger = logtest::start();
    test_server_listen(&mut logger).await;
    test_only_log_once(&mut logger).await?;
    Ok(())
}

async fn test_server_listen(logger: &mut logtest::Logger) {
    let port = test_utils::find_port().await;
    let app = tide::new();
    let res = app
        .listen(("127.0.0.1", port))
        .timeout(Duration::from_millis(60))
        .await;
    assert!(res.is_err());

    let record = logger
        .find(|rec| rec.args().starts_with("Server listening"))
        .unwrap();
    assert_eq!(
        record.args(),
        format!("Server listening on http://127.0.0.1:{}", port)
    );
}

async fn test_only_log_once(logger: &mut logtest::Logger) -> tide::Result<()> {
    let mut app = tide::new();
    app.with(tide::log::LogMiddleware::new());
    app.at("/").nest({
        let mut app = tide::new();
        app.at("/").get(|_| async { Ok("nested") });
        app
    });
    assert!(app.get("/").await?.status().is_success());

    let entries: Vec<_> = logger.collect();

    assert_eq!(
        1,
        entries
            .iter()
            .filter(|entry| entry.args() == "<-- Request received")
            .count()
    );

    assert_eq!(
        1,
        entries
            .iter()
            .filter(|entry| entry.args() == "--> Response sent")
            .count()
    );
    Ok(())
}
