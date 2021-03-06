use super::Collector;
use crate::deferred::Deferred;
use std::marker::PhantomData;

pub struct Shield<'collector> {
    collector: &'collector Collector,
    _m0: PhantomData<*mut ()>,
}

impl<'collector> Shield<'collector> {
    pub(crate) fn new(collector: &'collector Collector) -> Self {
        unsafe {
            collector.thread_state().enter(collector);
        }

        Self {
            collector,
            _m0: PhantomData,
        }
    }

    pub fn collector(&self) -> &'collector Collector {
        self.collector
    }

    pub fn repin(&mut self) {
        unsafe {
            self.collector.thread_state().exit(self.collector);
            self.collector.thread_state().enter(self.collector);
        }
    }

    pub fn repin_after<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        unsafe {
            self.collector.thread_state().exit(self.collector);
            let value = f();
            self.collector.thread_state().enter(self.collector);
            value
        }
    }

    pub fn retire<F: FnOnce() + 'collector>(&self, f: F) {
        let deferred = Deferred::new(f);
        self.collector.retire(deferred);
    }
}

impl<'collector> Clone for Shield<'collector> {
    fn clone(&self) -> Self {
        Self::new(self.collector)
    }
}

impl<'collector> Drop for Shield<'collector> {
    fn drop(&mut self) {
        unsafe {
            self.collector.thread_state().exit(self.collector);
        }
    }
}

#[derive(Clone)]
pub enum CowShield<'collector, 'shield> {
    Owned(Shield<'collector>),
    Borrowed(&'shield Shield<'collector>),
}

impl<'collector, 'shield> CowShield<'collector, 'shield> {
    pub fn new_owned(shield: Shield<'collector>) -> Self {
        Self::Owned(shield)
    }

    pub fn new_borrowed(shield: &'shield Shield<'collector>) -> Self {
        Self::Borrowed(shield)
    }

    pub fn into_owned(self) -> Shield<'collector> {
        match self {
            Self::Owned(shield) => shield,
            Self::Borrowed(shield) => shield.clone(),
        }
    }

    pub fn get(&self) -> &Shield<'collector> {
        match self {
            Self::Owned(shield) => shield,
            Self::Borrowed(shield) => shield,
        }
    }
}
