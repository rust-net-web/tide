use std::io::Error as IoError;
use std::path::Path;
use std::sync::Arc;

use async_std::{fs::OpenOptions, io};
use tempfile::TempDir;
use tide::prelude::*;
use tide::{Body, Request, Response, StatusCode};

#[derive(Clone)]
struct TempDirState {
    tempdir: Arc<TempDir>,
}

impl TempDirState {
    fn try_new() -> Result<Self, IoError> {
        Ok(Self {
            tempdir: Arc::new(tempfile::tempdir()?),
        })
    }

    fn path(&self) -> &Path {
        self.tempdir.path()
    }
}

#[async_std::main]
async fn main() -> Result<(), IoError> {
    tide::log::start();
    let mut app = tide::with_state(TempDirState::try_new()?);
    app.with(tide::log::LogMiddleware::new());

    // To test this example:
    // $ cargo run --example upload
    // $ curl -T ./README.md localhost:8080 # this writes the file to a temp directory
    // $ curl localhost:8080/README.md # this reads the file from the same temp directory

    app.at(":file")
        .put(|req: Request<TempDirState>| async move {
            let path = req.param("file")?;
            let fs_path = req.state().path().join(path);

            let file = OpenOptions::new()
                .create(true)
                .write(true)
                .open(&fs_path)
                .await?;

            let bytes_written = io::copy(req, file).await?;

            tide::log::info!("file written", {
                bytes: bytes_written,
                path: fs_path.canonicalize()?.to_str()
            });

            Ok(json!({ "bytes": bytes_written }))
        })
        .get(|req: Request<TempDirState>| async move {
            let path = req.param("file")?;
            let fs_path = req.state().path().join(path);

            if let Ok(body) = Body::from_file(fs_path).await {
                Ok(body.into())
            } else {
                Ok(Response::new(StatusCode::NotFound))
            }
        });

    app.listen("127.0.0.1:8080").await?;
    Ok(())
}
