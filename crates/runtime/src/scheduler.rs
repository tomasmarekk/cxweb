//! Bounded FIFO admission; leases are released before waiting for Codex tools.
use cxweb_domain::SessionKey;
use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub struct Scheduler {
    generation: Arc<Semaphore>,
    admission: Arc<Semaphore>,
    sessions: Arc<Mutex<HashSet<SessionKey>>>,
    queue_timeout: Duration,
}

impl Default for Scheduler {
    fn default() -> Self {
        // The current browser owner serializes DOM operations. Preparing a
        // second tab can block observation of the first beyond its deadline.
        // Queue independent native turns (including automatic titles) until a
        // driver with bounded interleaving is separately qualified.
        Self::new(1, 8, Duration::from_secs(600))
    }
}
impl Scheduler {
    fn new(active: usize, queued: usize, queue_timeout: Duration) -> Self {
        Self {
            generation: Arc::new(Semaphore::new(active)),
            admission: Arc::new(Semaphore::new(active + queued)),
            sessions: Arc::default(),
            queue_timeout,
        }
    }
    pub async fn acquire(
        &self,
        session: SessionKey,
        cancel: &CancellationToken,
    ) -> Result<GenerationLease, &'static str> {
        if cancel.is_cancelled() {
            return Err("E_CANCELLED");
        }
        let admission = self
            .admission
            .clone()
            .try_acquire_owned()
            .map_err(|_| "E_QUEUE_FULL")?;
        {
            let mut sessions = self.sessions.lock().map_err(|_| "E_RUNTIME_STATE")?;
            if !sessions.insert(session.clone()) {
                return Err("E_SESSION_BUSY");
            }
        }
        // This guard also cleans up when the acquire future itself is dropped.
        let session = SessionLease {
            key: session,
            sessions: self.sessions.clone(),
        };
        let generation = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err("E_CANCELLED"),
            result = tokio::time::timeout(self.queue_timeout, self.generation.clone().acquire_owned()) => {
                result.map_err(|_| "E_QUEUE_TIMEOUT")?.map_err(|_| "E_RUNTIME_SHUTDOWN")?
            }
        };
        Ok(GenerationLease {
            _generation: generation,
            _admission: admission,
            _session: session,
        })
    }
}

pub struct GenerationLease {
    _generation: OwnedSemaphorePermit,
    _admission: OwnedSemaphorePermit,
    _session: SessionLease,
}
struct SessionLease {
    key: SessionKey,
    sessions: Arc<Mutex<HashSet<SessionKey>>>,
}
impl Drop for SessionLease {
    fn drop(&mut self) {
        if let Ok(mut sessions) = self.sessions.lock() {
            sessions.remove(&self.key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn session(id: &str) -> SessionKey {
        SessionKey {
            installation: "i".into(),
            native_session: id.into(),
            account_scope: "a".into(),
            workspace_scope: "w".into(),
            route: "webbridge/test".into(),
            epoch: 0,
        }
    }
    #[tokio::test]
    async fn default_queues_auxiliary_turn_until_active_generation_releases() {
        let scheduler = Scheduler::default();
        let cancel = CancellationToken::new();
        let active = scheduler.acquire(session("main"), &cancel).await.unwrap();
        let auxiliary = scheduler.acquire(session("title"), &cancel);
        tokio::pin!(auxiliary);
        assert!(
            tokio::time::timeout(Duration::from_millis(20), &mut auxiliary)
                .await
                .is_err()
        );
        drop(active);
        let admitted = tokio::time::timeout(Duration::from_secs(1), auxiliary)
            .await
            .unwrap()
            .unwrap();
        drop(admitted);
        assert!(scheduler.acquire(session("main"), &cancel).await.is_ok());
    }
    #[tokio::test]
    async fn cancel_queued_work_releases_its_slot_and_session() {
        let scheduler = Scheduler::new(1, 1, Duration::from_secs(1));
        let cancel = CancellationToken::new();
        let active = scheduler.acquire(session("one"), &cancel).await.unwrap();
        let queue_cancel = CancellationToken::new();
        let queued_scheduler = scheduler.clone();
        let queued_cancel = queue_cancel.clone();
        let queued = tokio::spawn(async move {
            queued_scheduler
                .acquire(session("two"), &queued_cancel)
                .await
        });
        tokio::task::yield_now().await;
        assert!(matches!(
            scheduler.acquire(session("three"), &cancel).await,
            Err("E_QUEUE_FULL")
        ));
        queue_cancel.cancel();
        assert!(matches!(queued.await.unwrap(), Err("E_CANCELLED")));
        drop(active);
        assert!(scheduler.acquire(session("two"), &cancel).await.is_ok());
    }
    #[tokio::test]
    async fn releasing_generation_for_tool_results_allows_next_work() {
        let scheduler = Scheduler::new(1, 1, Duration::from_millis(100));
        let cancel = CancellationToken::new();
        let active = scheduler.acquire(session("parent"), &cancel).await.unwrap();
        assert!(matches!(
            scheduler.acquire(session("parent"), &cancel).await,
            Err("E_SESSION_BUSY")
        ));
        drop(active);
        let child = scheduler.acquire(session("child"), &cancel).await.unwrap();
        drop(child);
        assert!(scheduler.acquire(session("parent"), &cancel).await.is_ok());
    }
    #[tokio::test]
    async fn timeout_removes_session_and_queued_admission() {
        let scheduler = Scheduler::new(1, 1, Duration::from_millis(5));
        let cancel = CancellationToken::new();
        let active = scheduler.acquire(session("one"), &cancel).await.unwrap();
        assert!(matches!(
            scheduler.acquire(session("two"), &cancel).await,
            Err("E_QUEUE_TIMEOUT")
        ));
        drop(active);
        assert!(scheduler.acquire(session("two"), &cancel).await.is_ok());
    }
}
