use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

#[derive(Debug)]
pub struct CancelationToken {
	shared_state: Arc<Mutex<CancelationTokenState>>
}

#[derive(Debug)]
struct CancelationTokenState {
	canceled: bool,
	waker: Option<Waker>
}

#[derive(Debug)]
pub struct ReturnOnCancel<T> {
    result: Mutex<RefCell<Option<T>>>,
	shared_state: Arc<Mutex<CancelationTokenState>>
}

/// Future that allows gracefully shutting down the server
impl CancelationToken {
	pub fn new() -> CancelationToken {
		CancelationToken {
			shared_state: Arc::new(Mutex::new(CancelationTokenState {
				canceled: false,
				waker: None
			}))
		}
	}

	/// Call to shut down the server
	pub fn complete(&self) {
		let mut shared_state = self.shared_state.lock().unwrap();

		shared_state.canceled = true;
		if let Some(waker) = shared_state.waker.take() {
			waker.wake()
		}
    }
    
    pub fn return_on_cancel<T>(&self, result: T) -> ReturnOnCancel<T> {
        ReturnOnCancel {
            result: Mutex::new(RefCell::new(Some(result))),
            shared_state: self.shared_state.clone()
        }
    }
}

impl<T> Future for ReturnOnCancel<T> {
    type Output = T;

	fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
		let mut shared_state = self.shared_state.lock().unwrap();

		if shared_state.canceled {
            let result_refcell = self.result.lock().unwrap();
            let result = result_refcell.replace(None).expect("Result was already returned");
            Poll::Ready(result)
		} else {
            shared_state.waker = Some(cx.waker().clone());
            Poll::Pending
		}
	}
}

impl Clone for CancelationToken {
	fn clone(&self) -> Self {
		CancelationToken {
			shared_state: self.shared_state.clone()
		}
	}
} 