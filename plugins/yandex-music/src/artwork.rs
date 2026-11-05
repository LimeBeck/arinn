// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded, account-scoped single-flight artwork. A short RPC never owns a fetch.
use arinn_protocol::{assets::Asset, validation::ErrorCode};
use std::{
    collections::VecDeque,
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::sync::{Mutex, Semaphore, watch};
use tokio::time::Instant;
type Image = Result<Arc<Asset>, ErrorCode>;
struct Entry {
    epoch: u64,
    id: String,
    created: Instant,
    result: watch::Sender<Option<Image>>,
}
pub(crate) struct Artwork {
    epoch: AtomicU64,
    entries: Mutex<VecDeque<Entry>>,
    slots: Arc<Semaphore>,
}
impl Artwork {
    pub(crate) fn new(slots: Arc<Semaphore>) -> Arc<Self> {
        Arc::new(Self {
            epoch: AtomicU64::new(0),
            entries: Mutex::new(VecDeque::new()),
            slots,
        })
    }
    pub(crate) async fn clear(&self, epoch: u64) {
        let mut entries = self.entries.lock().await;
        if epoch >= self.epoch.load(Ordering::Relaxed) {
            self.epoch.store(epoch, Ordering::Relaxed);
            entries.clear();
        }
    }
    pub(crate) async fn get<F>(
        self: &Arc<Self>,
        epoch: u64,
        id: String,
        work: F,
    ) -> Result<Asset, ErrorCode>
    where
        F: Future<Output = Result<Asset, ErrorCode>> + Send + 'static,
    {
        let receiver = {
            let mut entries = self.entries.lock().await;
            let current = self.epoch.load(Ordering::Relaxed);
            if epoch < current {
                return Err(ErrorCode::Conflict);
            }
            if epoch > current {
                self.epoch.store(epoch, Ordering::Relaxed);
                entries.clear();
            }
            entries.retain(|e| e.epoch == epoch && e.created.elapsed() < Duration::from_secs(60));
            if let Some(index) = entries.iter().position(|e| e.id == id) {
                let entry = entries.remove(index).unwrap();
                let receiver = entry.result.subscribe();
                entries.push_back(entry);
                receiver
            } else {
                let permit = self
                    .slots
                    .clone()
                    .try_acquire_owned()
                    .map_err(|_| ErrorCode::Unavailable)?;
                while entries.len() >= 16 {
                    let Some(index) = entries.iter().position(|e| e.result.borrow().is_some())
                    else {
                        return Err(ErrorCode::Unavailable);
                    };
                    entries.remove(index);
                }
                let (sender, receiver) = watch::channel(None);
                entries.push_back(Entry {
                    epoch,
                    id,
                    created: Instant::now(),
                    result: sender.clone(),
                });
                let owner = self.clone();
                tokio::spawn(async move {
                    let _permit = permit;
                    let image = tokio::time::timeout(Duration::from_secs(30), work)
                        .await
                        .unwrap_or(Err(ErrorCode::Timeout))
                        .and_then(|asset| {
                            asset.validate()?;
                            Ok(Arc::new(asset))
                        });
                    let mut entries = owner.entries.lock().await;
                    // A reconnect/clear must never repopulate the retired cache.
                    if entries.iter().any(|e| e.result.same_channel(&sender)) {
                        sender.send_replace(Some(image));
                        while entries
                            .iter()
                            .map(|e| {
                                e.result
                                    .borrow()
                                    .as_ref()
                                    .and_then(|r| r.as_ref().ok())
                                    .map_or(0, |a| a.data.len())
                            })
                            .sum::<usize>()
                            > 16 * 1024 * 1024
                        {
                            let Some(index) =
                                entries.iter().position(|e| e.result.borrow().is_some())
                            else {
                                break;
                            };
                            entries.remove(index);
                        }
                    } else {
                        sender.send_replace(Some(Err(ErrorCode::Conflict)));
                    }
                });
                receiver
            }
        };
        let mut receiver = receiver;
        let image =
            tokio::time::timeout(Duration::from_secs(4), receiver.wait_for(|r| r.is_some()))
                .await
                .map_err(|_| ErrorCode::Timeout)?
                .map_err(|_| ErrorCode::Unavailable)?
                .as_ref()
                .unwrap()
                .clone()?;
        Ok((*image).clone())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn asset() -> Asset {
        Asset {
            mime: "image/jpeg".into(),
            data: vec![255, 216, 255, 0],
        }
    }
    #[tokio::test(start_paused = true)]
    async fn a_fifth_cover_can_load_after_the_first_four_slow_workers_release_admission() {
        let cache = Artwork::new(Arc::new(Semaphore::new(4)));
        let held = cache.slots.clone().acquire_many_owned(4).await.unwrap();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(25)).await;
            drop(held);
        });
        let mut loaded = false;
        for delay in [0, 1, 2, 4, 8, 16] {
            tokio::time::sleep(Duration::from_secs(delay)).await;
            match cache.get(1, "cover:5".into(), async { Ok(asset()) }).await {
                Ok(image) => {
                    assert_eq!(image, asset());
                    loaded = true;
                    break;
                }
                Err(error) => assert_eq!(error, ErrorCode::Unavailable),
            }
        }
        assert!(
            loaded,
            "visible retries must outlast a slow admission batch"
        );
    }
    #[tokio::test]
    async fn cache_budget_and_account_epoch_are_enforced_for_ready_images() {
        let cache = Artwork::new(Arc::new(Semaphore::new(4)));
        for i in 0..3 {
            cache
                .get(1, format!("cover:{i}"), async {
                    let mut a = asset();
                    a.data.resize(8 * 1024 * 1024, 0);
                    Ok(a)
                })
                .await
                .unwrap();
        }
        assert_eq!(cache.entries.lock().await.len(), 2);
        cache.clear(2).await;
        assert_eq!(
            cache
                .get(1, "cover:1".into(), async {
                    panic!("old account must not fetch")
                })
                .await,
            Err(ErrorCode::Conflict)
        );
    }
    #[tokio::test(start_paused = true)]
    async fn caller_timeout_does_not_cancel_shared_fetch_and_reconnect_retires_it() {
        let cache = Artwork::new(Arc::new(Semaphore::new(1)));
        assert_eq!(
            cache
                .get(1, "cover:1".into(), async {
                    tokio::time::sleep(Duration::from_secs(6)).await;
                    Ok(asset())
                })
                .await,
            Err(ErrorCode::Timeout)
        );
        let cached = cache
            .get(1, "cover:1".into(), async {
                panic!("duplicate network request")
            })
            .await
            .unwrap();
        assert_eq!(cached, asset());
        cache.clear(2).await;
        assert_eq!(
            cache
                .get(2, "cover:1".into(), async { Err(ErrorCode::NotFound) })
                .await,
            Err(ErrorCode::NotFound)
        );
    }
    #[tokio::test(start_paused = true)]
    async fn cancellation_leaves_one_bounded_worker_and_clear_never_republishes_it() {
        let cache = Artwork::new(Arc::new(Semaphore::new(1)));
        let copy = cache.clone();
        let caller = tokio::spawn(async move {
            copy.get(1, "cover:1".into(), async {
                tokio::time::sleep(Duration::from_secs(8)).await;
                Ok(asset())
            })
            .await
        });
        tokio::task::yield_now().await;
        caller.abort();
        cache.clear(2).await;
        tokio::time::sleep(Duration::from_secs(9)).await;
        assert!(cache.entries.lock().await.is_empty());
        assert_eq!(cache.slots.available_permits(), 1);
        assert_eq!(
            cache
                .get(2, "cover:1".into(), async { Ok(asset()) })
                .await
                .unwrap(),
            asset()
        );
    }
}
