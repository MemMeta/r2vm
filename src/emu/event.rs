//! This module handles event-driven simulation

use std::collections::BinaryHeap;
use std::sync::Mutex;

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use std::sync::Condvar;
use std::time::{Duration, Instant};

struct Entry {
    time: u64,
    handler: Box<FnOnce()>,
}

// #region Ordering relation for Entry
//

impl PartialEq for Entry {
    fn eq(&self, other: &Self) -> bool {
        self.time == other.time
    }
}

impl Eq for Entry {}

impl PartialOrd for Entry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Entry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Smaller time needs to come larger as BinaryHeap is a max-heap.
        other.time.cmp(&self.time)
    }
}

//
// #endregion

#[repr(C)]
pub struct EventLoop {
    // Only used in non-threaded mode
    cycle: AtomicU64,
    next_event: AtomicU64,
    // Only used in threaded mode
    epoch: Instant,
    condvar: Condvar,
    // This has to be a Box to allow repr(C)
    events: Mutex<BinaryHeap<Entry>>,
    shutdown: AtomicBool,
}

extern {
    // See also `event.s` for this function
    fn event_loop_wait();
}

impl EventLoop {
    /// Create a new event loop.
    pub fn new() -> EventLoop {
        EventLoop {
            cycle: AtomicU64::new(0),
            next_event: AtomicU64::new(u64::max_value()),
            epoch: Instant::now(),
            condvar: Condvar::new(),
            events: Mutex::new(BinaryHeap::new()),
            shutdown: AtomicBool::new(false),
        }
    }

    /// Creata a fiber for the event loop
    pub fn create_fiber(self) -> crate::fiber::Fiber {
        let event_fiber = crate::fiber::Fiber::new();
        let ptr = event_fiber.data_pointer();
        unsafe { std::ptr::write(ptr, self) }
        event_fiber.set_fn(event_loop);
        event_fiber
    }

    /// Stop this event loop.
    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
        // Queue a no-op event to wake the loop up.
        self.queue(0, Box::new(|| {}));
    }

    /// Query the current cycle count.
    pub fn cycle(&self) -> u64 {
        if cfg!(feature = "thread") {
            let duration = Instant::now().duration_since(self.epoch);
            duration.as_micros() as u64 * 100
        } else {
            self.cycle.load(Ordering::Relaxed)
        }
    }

    /// Add a new event to the event loop for triggering. If it happens in the past it will be
    /// dequeued and triggered as soon as `cycle` increments for the next time.
    pub fn queue(&self, cycle: u64, handler: Box<FnOnce()>) {
        let mut guard = self.events.lock().unwrap();
        guard.push(Entry {
            time: cycle,
            handler,
        });

        if cfg!(feature = "thread") {
            // If the event just queued is the next event, we need to wake the event loop up.
            if guard.peek().unwrap().time == cycle {
                self.condvar.notify_one()
            }
        } else {
            // It's okay to be relaxed because guard's release op will order it.
            self.next_event.store(match guard.peek() {
                Some(it) => it.time,
                None => u64::max_value(),
            }, Ordering::Relaxed);
        }
    }

    /// Query the current time (we pretend to be operating at 100MHz at the moment)
    pub fn time(&self) -> u64 {
        self.cycle() / 100
    }

    pub fn queue_time(&self, time: u64, handler: Box<FnOnce()>) {
        self.queue(time * 100, handler);
    }

    /// Handle all events at or before `cycle`, and return the cycle of next event if any.
    fn handle_events(&self, guard: &mut std::sync::MutexGuard<BinaryHeap<Entry>>, cycle: u64) -> Option<u64> {
        loop {
            let time = match guard.peek() {
                None => return None,
                Some(v) => v.time,
            };
            if time > cycle {
                return Some(time)
            }
            let entry = guard.pop().unwrap();
            (entry.handler)();
        }
    }
}

pub fn event_loop() {
    let this: &EventLoop = unsafe { &*crate::fiber::Fiber::scratchpad() };
    let mut guard = this.events.lock().unwrap();
    loop {
        let cycle = this.cycle();
        if this.shutdown.load(Ordering::Relaxed) {
            return;
        }
        let result = this.handle_events(&mut guard, cycle);
        if cfg!(feature = "thread") {
            guard = match result {
                None => this.condvar.wait(guard).unwrap(),
                Some(v) => this.condvar.wait_timeout(guard, Duration::from_micros(v - cycle)).unwrap().0,
            }
        } else {
            this.next_event.store(result.unwrap_or(u64::max_value()), Ordering::Relaxed);
            std::mem::drop(guard);
            unsafe { event_loop_wait() }
            guard = this.events.lock().unwrap();
        }
    }
}
