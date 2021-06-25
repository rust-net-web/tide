//! Future and Stream cancellation

use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};

use async_std::future::Future;
use async_std::stream::Stream;
use async_std::sync::Arc;
use async_std::task::{Context, Poll};

use event_listener::{Event, EventListener};
use pin_project_lite::pin_project;

/// StopSource produces [StopToken] and cancels all of its tokens on drop.
#[derive(Debug)]
pub struct StopSource {
    stopped: Arc<AtomicBool>,
    event: Arc<Event>,
}

impl StopSource {
    /// Create a new StopSource
    pub fn new() -> Self {
        Self {
            stopped: Arc::new(AtomicBool::new(false)),
            event: Arc::new(Event::new()),
        }
    }

    /// Produce a new [StopToken], associated with this source.
    ///
    /// Once this source is dropped, all associated [StopToken] futures will complete.
    pub fn token(&self) -> StopToken {
        StopToken {
            stopped: self.stopped.clone(),
            event_listener: self.event.listen(),
            event: self.event.clone(),
        }
    }
}

impl Default for StopSource {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for StopSource {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::SeqCst);
        self.event.notify(usize::MAX);
    }
}

pin_project! {
    /// StopToken is a future which completes when the associated [StopSource] is dropped.
    #[derive(Debug)]
    pub struct StopToken {
        #[pin]
        stopped: Arc<AtomicBool>,
        #[pin]
        event_listener: EventListener,
        event: Arc<Event>,
    }
}

impl StopToken {
    /// Produce a StopToken that associates with no [StopSource], and never completes.
    pub fn never() -> Self {
        let event = Event::new();
        Self {
            stopped: Arc::new(AtomicBool::new(false)),
            event_listener: event.listen(),
            event: Arc::new(event),
        }
    }
}

impl Clone for StopToken {
    fn clone(&self) -> Self {
        Self {
            stopped: self.stopped.clone(),
            event_listener: self.event.listen(),
            event: self.event.clone(),
        }
    }
}

impl Future for StopToken {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.project();
        let _ = Future::poll(Pin::new(&mut this.event_listener), cx);
        if this.stopped.load(Ordering::Relaxed) {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}

pin_project! {
    /// A stream that early exits when inner [StopToken] completes.
    ///
    /// Users usually do not need to construct this type manually, but rather use the [StopStreamExt::stop_on] method instead.
    #[derive(Debug)]
    pub struct StopStream<S> {
        #[pin]
        stream: S,
        #[pin]
        stop_token: StopToken,
    }
}

impl<S> StopStream<S> {
    /// Wraps a stream to exit early when `stop_token` completes.
    pub fn new(stream: S, stop_token: StopToken) -> Self {
        Self { stream, stop_token }
    }
}

impl<S> Stream for StopStream<S>
where
    S: Stream,
{
    type Item = S::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();
        if Future::poll(Pin::new(&mut this.stop_token), cx).is_ready() {
            Poll::Ready(None)
        } else {
            this.stream.poll_next(cx)
        }
    }
}

/// Stream extensions to generate [StopStream] that exits early when `stop_token` completes.
pub trait StopStreamExt: Sized {
    /// Wraps a stream to exit early when `stop_token` completes.
    fn stop_on(self, stop_token: StopToken) -> StopStream<Self> {
        StopStream::new(self, stop_token)
    }
}

impl<S> StopStreamExt for S where S: Stream {}

pin_project! {
    /// A future that early exits when inner [StopToken] completes.
    ///
    /// Users usually do not need to construct this type manually, but rather use the [StopFutureExt::stop_on] method instead.
    #[derive(Debug)]
    pub struct StopFuture<F> {
        #[pin]
        future: F,
        #[pin]
        stop_token: StopToken,
    }
}

impl<F> StopFuture<F> {
    /// Wraps a future to exit early when `stop_token` completes.
    pub fn new(future: F, stop_token: StopToken) -> Self {
        Self { future, stop_token }
    }
}

impl<F> Future for StopFuture<F>
where
    F: Future,
{
    type Output = Option<F::Output>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.project();
        if Future::poll(Pin::new(&mut this.stop_token), cx).is_ready() {
            Poll::Ready(None)
        } else {
            match this.future.poll(cx) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(output) => Poll::Ready(Some(output)),
            }
        }
    }
}

/// Future extensions to generate [StopFuture] that exits early when `stop_token` completes.
pub trait StopFutureExt: Sized {
    /// Wraps a future to exit early when `stop_token` completes.
    fn stop_on(self, stop_token: StopToken) -> StopFuture<Self> {
        StopFuture::new(self, stop_token)
    }
}

impl<F> StopFutureExt for F where F: Future {}

#[cfg(test)]
mod tests {
    use std::thread;
    use std::time::Duration;

    use async_std::prelude::{FutureExt, StreamExt};

    use super::*;

    #[test]
    fn test_cancellation() {
        let source = StopSource::new();
        let stop_token = source.token();

        let pending_stream1 = async_std::stream::pending::<()>();
        let pending_stream2 = async_std::stream::pending::<()>();
        let pending_future1 = async_std::future::pending::<()>();
        let pending_future2 = async_std::future::pending::<()>();
        let wrapped_stream1 = pending_stream1.stop_on(stop_token.clone());
        let wrapped_stream2 = pending_stream2.stop_on(stop_token.clone());
        let wrapped_future1 = pending_future1.stop_on(stop_token.clone());
        let wrapped_future2 = pending_future2.stop_on(stop_token);

        let join_future = wrapped_stream1
            .last()
            .join(wrapped_stream2.last())
            .join(wrapped_future1)
            .join(wrapped_future2);

        thread::spawn(move || {
            let source = source;
            thread::sleep(Duration::from_secs(1));
            drop(source);
        });

        let res = async_std::task::block_on(join_future);
        assert_eq!(res, (((None, None), None), None));
    }

    #[test]
    fn test_never() {
        let pending_future = async_std::future::pending::<()>();
        let wrapped_future = pending_future.stop_on(StopToken::never());

        let res = async_std::task::block_on(wrapped_future.timeout(Duration::from_secs(1)));
        assert!(res.is_err());
    }
}
