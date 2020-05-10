//! Implementation of parking lot for fibers
//!
//! For detailed implementation, check out the following URLs
//! * https://webkit.org/blog/6161/locking-in-webkit/
//! * https://docs.rs/parking_lot

use super::raw::fiber_sleep;
use super::{fiber_current, FiberGroup, FiberStack};
use lazy_static::lazy_static;
use std::ptr::NonNull;

#[derive(Clone, Copy)]
struct WaitEntry {
    fiber: FiberStack,
    next: Option<NonNull<WaitEntry>>,
}

struct WaitList {
    head: NonNull<WaitEntry>,
    tail: NonNull<WaitEntry>,
}

unsafe impl Send for WaitList {}

lazy_static! {
    static ref WAIT_LIST_MAP: super::map::ConcurrentMap<usize, WaitList> =
        super::map::ConcurrentMap::new();
}

pub fn park(key: usize, validate: impl FnOnce() -> bool, before_sleep: impl FnOnce()) {
    // Required before calling fiber_current.
    super::assert_in_fiber();

    let cur = unsafe { fiber_current() };
    let mut entry = WaitEntry { fiber: cur, next: None };

    let valid = WAIT_LIST_MAP.with(key, |list| {
        // Deadlock prevention: must acquire group lock after list lock.

        // Give the caller a chance, under strong synchronisation guarantee, to do last check and possibly abort.
        if !validate() {
            return false;
        }

        match list {
            None => {
                *list = Some(WaitList { head: (&mut entry).into(), tail: (&mut entry).into() });
            }
            Some(ref mut list) => {
                unsafe { list.tail.as_mut().next = Some((&mut entry).into()) };
                list.tail = (&mut entry).into();
            }
        }

        unsafe { FiberGroup::prepare_pause(cur) };
        true
    });

    if !valid {
        return;
    }

    before_sleep();

    unsafe {
        let awaken = FiberGroup::pause(cur);
        if !awaken {
            fiber_sleep(0);
        }
    };
}

pub fn unpark_all(key: usize) {
    let list = WAIT_LIST_MAP.with(key, |list| list.take());
    if let Some(list) = list {
        let mut ptr = Some(list.head);
        while let Some(mut entry) = ptr {
            let entry = unsafe { entry.as_mut() };
            unsafe { FiberGroup::unpause(entry.fiber) };
            ptr = entry.next;
        }
    }
}

pub fn unpark_one(key: usize, callback: impl FnOnce(bool)) {
    let fiber = WAIT_LIST_MAP.with(key, |list| {
        let ret = if let Some(ref mut inner) = list {
            let entry = unsafe { &mut *inner.head.as_ptr() };
            match entry.next {
                None => *list = None,
                Some(next) => inner.head = next,
            }
            Some(entry.fiber)
        } else {
            None
        };
        callback(list.is_some());
        ret
    });
    if let Some(fiber) = fiber {
        unsafe { FiberGroup::unpause(fiber) };
    }
}
