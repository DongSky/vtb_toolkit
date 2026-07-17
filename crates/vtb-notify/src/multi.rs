//! Fan-out delivery to every configured channel.

use crate::{Notifier, NotifyEvent};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Sends each event through every wrapped notifier concurrently. One channel
/// failing is logged with `tracing::warn!` but never blocks the others.
pub struct MultiNotifier(pub Vec<Box<dyn Notifier>>);

impl MultiNotifier {
    pub fn new(notifiers: Vec<Box<dyn Notifier>>) -> Self {
        Self(notifiers)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn push(&mut self, notifier: Box<dyn Notifier>) {
        self.0.push(notifier);
    }

    /// Deliver `ev` on all channels concurrently; returns how many succeeded.
    pub async fn notify_all<'a>(&'a self, ev: &'a NotifyEvent) -> usize {
        let pending: Vec<Option<BoxFut<'a>>> = self
            .0
            .iter()
            .map(|n| {
                let fut: BoxFut<'a> = Box::pin(async move {
                    match n.notify(ev).await {
                        Ok(()) => true,
                        Err(e) => {
                            tracing::warn!(notifier = n.name(), error = %e, "notification failed");
                            false
                        }
                    }
                });
                Some(fut)
            })
            .collect();

        JoinAll {
            pending,
            succeeded: 0,
        }
        .await
    }
}

type BoxFut<'a> = Pin<Box<dyn Future<Output = bool> + Send + 'a>>;

/// Minimal `join_all` so we don't pull in the `futures` crate. Re-polls every
/// still-pending future on each wake — fine for a handful of notifiers.
struct JoinAll<'a> {
    pending: Vec<Option<BoxFut<'a>>>,
    succeeded: usize,
}

impl Future for JoinAll<'_> {
    type Output = usize;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<usize> {
        let this = self.get_mut();
        let mut all_done = true;
        for slot in &mut this.pending {
            if let Some(fut) = slot {
                match fut.as_mut().poll(cx) {
                    Poll::Ready(ok) => {
                        if ok {
                            this.succeeded += 1;
                        }
                        *slot = None;
                    }
                    Poll::Pending => all_done = false,
                }
            }
        }
        if all_done {
            Poll::Ready(this.succeeded)
        } else {
            Poll::Pending
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NotifyError, NotifyKind, Result};
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    struct TestNotifier {
        fail: bool,
        calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl Notifier for TestNotifier {
        async fn notify(&self, _ev: &NotifyEvent) -> Result<()> {
            // Yield so the joined futures actually interleave.
            tokio::task::yield_now().await;
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.fail {
                Err(NotifyError::Delivery("boom".into()))
            } else {
                Ok(())
            }
        }

        fn name(&self) -> &str {
            "test"
        }
    }

    #[tokio::test]
    async fn partial_failure_still_counts_the_rest() {
        let calls = Arc::new(AtomicUsize::new(0));
        let multi = MultiNotifier(vec![
            Box::new(TestNotifier {
                fail: false,
                calls: calls.clone(),
            }),
            Box::new(TestNotifier {
                fail: true,
                calls: calls.clone(),
            }),
            Box::new(TestNotifier {
                fail: false,
                calls: calls.clone(),
            }),
        ]);
        let ev = NotifyEvent::new(NotifyKind::RecordingError, "录制出错", "磁盘已满");
        assert_eq!(multi.notify_all(&ev).await, 2);
        // Every notifier was attempted despite the failure in the middle.
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn empty_multi_returns_zero() {
        let multi = MultiNotifier::new(vec![]);
        assert!(multi.is_empty());
        let ev = NotifyEvent::new(NotifyKind::Custom, "t", "b");
        assert_eq!(multi.notify_all(&ev).await, 0);
    }
}
