mod test_utils;
use async_std::prelude::*;
use async_std::task;
use std::time::Duration;

use tide::stopper::Stopper;
use tide::Response;

#[async_std::test]
async fn cancellation() -> Result<(), http_types::Error> {
    let port = test_utils::find_port().await;
    let stopper = Stopper::new();
    let stopper_ = stopper.clone();

    let server = task::spawn(async move {
        let mut app = tide::new();
        app.with_stopper(stopper_);
        app.at("/").get(|_| async {
            task::sleep(Duration::from_secs(1)).await;
            Ok(Response::new(200))
        });
        app.listen(("localhost", port)).await?;
        tide::Result::Ok(())
    });

    let client1 = task::spawn(async move {
        task::sleep(Duration::from_millis(100)).await;
        let res = surf::get(format!("http://localhost:{}", port))
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        async_std::future::pending().await
    });

    let client2 = task::spawn(async move {
        task::sleep(Duration::from_millis(200)).await;
        let res = surf::get(format!("http://localhost:{}", port))
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        async_std::future::pending().await
    });

    let stop = task::spawn(async move {
        task::sleep(Duration::from_millis(300)).await;
        stopper.stop();
        Ok(())
    });

    server
        .try_join(stop)
        .race(client1.try_join(client2))
        .timeout(Duration::from_secs(2))
        .await??;

    Ok(())
}
