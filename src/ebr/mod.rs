mod epoch;
mod shield;
mod thread_state;

use crate::drain_queue::DrainQueue;
use crate::{deferred::Deferred, thread_local::ThreadLocal};
use epoch::{AtomicEpoch, Epoch};
pub use shield::{CowShield, Shield};
use std::sync::atomic::{AtomicUsize, Ordering};
use thread_state::{EbrState, ThreadState};

pub struct Collector {
    global_epoch: AtomicEpoch,
    pinned: AtomicUsize,
    threads: ThreadLocal<ThreadState<Self>>,
    deferred: [DrainQueue<Deferred>; Epoch::AMOUNT],
}

impl Collector {
    pub fn new() -> Self {
        Self {
            global_epoch: AtomicEpoch::new(Epoch::ZERO),
            pinned: AtomicUsize::new(0),
            threads: ThreadLocal::new(),
            deferred: [DrainQueue::new(), DrainQueue::new(), DrainQueue::new()],
        }
    }

    pub fn shield(&self) -> Shield {
        Shield::new(self)
    }

    pub fn collect(&self) {
        self.try_cycle();
    }

    pub(crate) fn retire(&self, deferred: Deferred) {
        let epoch = self.global_epoch.load(Ordering::Relaxed);
        self.get_queue(epoch).push(deferred);
    }

    fn get_queue(&self, epoch: Epoch) -> &DrainQueue<Deferred> {
        let raw_epoch = epoch.into_raw();
        unsafe { self.deferred.get_unchecked(raw_epoch) }
    }

    fn try_advance(&self) -> Result<Epoch, ()> {
        let global_epoch = self.global_epoch.load(Ordering::Relaxed);

        let can_collect = self
            .threads
            .iter()
            .map(|state| state.load_epoch_relaxed())
            .filter(|epoch| epoch.is_pinned())
            .all(|epoch| {
                epoch.unpinned() == global_epoch && self.pinned.load(Ordering::Acquire) == 0
            });

        if can_collect {
            self.global_epoch.try_advance(global_epoch)
        } else {
            Err(())
        }
    }

    unsafe fn internal_collect(&self, epoch: Epoch) {
        self.pinned.fetch_add(1, Ordering::Release);
        let mut queue = self.get_queue(epoch).swap_out();

        while let Some(deferred) = queue.pop() {
            deferred.call();
        }

        self.pinned.fetch_sub(1, Ordering::Release);
    }

    pub(crate) fn thread_state(&self) -> &ThreadState<Self> {
        self.threads.get(|| ThreadState::new())
    }
}

impl EbrState for Collector {
    fn load_epoch_relaxed(&self) -> Epoch {
        self.global_epoch.load(Ordering::Relaxed)
    }

    fn should_advance(&self) -> bool {
        let epoch = self.global_epoch.load(Ordering::Acquire);
        let queue = self.get_queue(epoch);
        queue.len() != 0
    }

    fn try_cycle(&self) {
        if let Ok(epoch) = self.try_advance() {
            let safe_epoch = epoch.next();

            unsafe {
                self.internal_collect(safe_epoch);
            }
        }
    }
}

impl Default for Collector {
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl Send for Collector {}
unsafe impl Sync for Collector {}
