//! Sync/async bridge: run a [`Future`] to completion from a synchronous context.
//!
//! The deep-worker runtime invokes [`ContextProvider`] methods on a blocking
//! thread.  Those methods are synchronous at the trait boundary, but the
//! underlying LLM calls are async.  [`block_on`] bridges the two worlds without
//! requiring the caller to carry a Tokio handle.

use std::future::Future;

/// Block on `fut`. If called inside a multi-thread Tokio runtime, uses
/// `block_in_place` + the current handle so the executor is not starved.
/// Otherwise a transient current-thread runtime is spun up for the duration.
///
/// # Panics
///
/// The one allowed `expect` in this function is when building a fresh
/// current-thread runtime fails — that is an unrecoverable system-level error
/// (e.g. exhausted OS thread handles).
///
/// Must NOT be called from within a current-thread runtime's async context
/// (i.e. inside `#[tokio::test(flavor = "current_thread")]` or an async block
/// running on a single-threaded executor), because `block_in_place` is a
/// no-op there.  The deep-worker invokes providers on a dedicated blocking
/// thread, so this constraint is always satisfied in production.
pub fn block_on<F: Future>(fut: F) -> F::Output {
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => tokio::task::block_in_place(|| handle.block_on(fut)),
        Err(_) => tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build transient current-thread runtime")
            .block_on(fut),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_on_runs_future() {
        assert_eq!(block_on(async { 1 + 1 }), 2);
    }

    #[test]
    fn block_on_nested_async_value() {
        let result = block_on(async {
            let a = async { 10u32 }.await;
            let b = async { 20u32 }.await;
            a + b
        });
        assert_eq!(result, 30);
    }
}
