//! Test-only helpers for GPU tests that must not hang the test binary.

use std::future::Future;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use futures::FutureExt;

/// Try to obtain a wgpu device and queue without blocking for longer than one poll.
///
/// Returns `None` when no adapter is available or the adapter/device request future
/// is not ready after a single poll.
pub(crate) fn device_and_queue() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = crate::offscreen::headless_instance();
    let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions::default()).now_or_never()?.ok()?;
    let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor::default()).now_or_never()?.ok()?;
    Some((device, queue))
}

/// One device for every GPU test in this binary, created once on a helper thread and given
/// [`SHARED_DEVICE_BUDGET`] to appear. `None` (tests skip) without an adapter or when the driver
/// does not answer in time. Sharing one device keeps parallel tests from each opening their own,
/// which is what made test binaries hang on a busy GPU.
pub(crate) fn shared_device() -> Option<(wgpu::Device, wgpu::Queue)> {
    static SHARED: std::sync::OnceLock<Option<(wgpu::Device, wgpu::Queue)>> = std::sync::OnceLock::new();
    SHARED.get_or_init(|| crate::offscreen::device_within(SHARED_DEVICE_BUDGET)).clone()
}

/// How long [`shared_device`] waits for the adapter and device.
pub(crate) const SHARED_DEVICE_BUDGET: Duration = Duration::from_secs(30);

/// Poll `fut` until it resolves or `budget` elapses, sleeping 5 ms between polls.
pub(crate) fn poll_bounded<F: Future>(fut: F, budget: Duration) -> Option<F::Output> {
    let mut fut = Box::pin(fut);
    let waker = futures::task::noop_waker();
    let mut cx = Context::from_waker(&waker);
    let start = Instant::now();
    loop {
        match fut.as_mut().poll(&mut cx) {
            Poll::Ready(output) => return Some(output),
            Poll::Pending => {}
        }
        if Instant::now().duration_since(start) >= budget {
            return None;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_device_is_created_once() {
        let start = Instant::now();
        let a = shared_device();
        let b = shared_device();
        assert_eq!(a.is_some(), b.is_some());
        assert!(start.elapsed() < SHARED_DEVICE_BUDGET + Duration::from_secs(5));
    }

    #[test]
    fn gputest_helper_returns_none_or_a_device_without_hanging() {
        let _ = device_and_queue();

        let start = Instant::now();
        let val = poll_bounded(async { 42u32 }, Duration::from_secs(1));
        assert_eq!(val, Some(42));
        assert!(start.elapsed() < Duration::from_secs(5));

        let start = Instant::now();
        let val = poll_bounded(futures::future::pending::<u32>(), Duration::from_millis(20));
        assert!(val.is_none());
        assert!(start.elapsed() >= Duration::from_millis(15));
    }
}
