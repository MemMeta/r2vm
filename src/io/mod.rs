pub mod clint;
pub mod console;
pub mod network;
pub mod plic;
pub mod rtc;
pub mod virtio;

/// IoMemory represents a region of physically continuous I/O memory.
///
/// We currently expect only one guest core can access a region of I/O memory at a time. Usually as
/// I/O access are rare and considered expensive, having a global lock for all I/O access, or have
/// a lock for each region of I/O memory should be sufficient. It somehow it ended up being a
/// bottleneck then we can come back to optimise it.
pub trait IoMemory {
    /// Read from I/O memory. `size` can be either 1, 2, 4 or 8. `addr` must be aligned properly,
    /// e.g. when `size` is 4, the least significant 2 bits of `addr` should be zero.
    ///
    /// Note that we use `&mut self`, as read operation is not always nullpotent in I/O memory.
    ///
    /// `addr` has type `usize`, which depends on the architecture. However, we believe that there
    /// is never a use-case for continuous memory region with more than 4GiB size on a 32-bit
    /// machine on 32-bit machine. Use `usize` over `u64` makes it much easier to handle indexes.
    fn read(&mut self, addr: usize, size: u32) -> u64;

    /// Write to I/O memory. `size` can be either 1, 2, 4 or 8. `addr` must be aligned properly,
    /// e.g. when `size` is 4, the least significant 2 bits of `addr` should be zero.
    fn write(&mut self, addr: usize, value: u64, size: u32);
}

/// An IoMemory which is synchronised internally, so it is suitable for multi-threaded access.
pub trait IoMemorySync: Send + Sync {
    fn read_sync(&self, addr: usize, size: u32) -> u64;
    fn write_sync(&self, addr: usize, value: u64, size: u32);
}

impl<T: IoMemorySync + ?Sized> IoMemory for T {
    fn read(&mut self, addr: usize, size: u32) -> u64 {
        self.read_sync(addr, size)
    }
    fn write(&mut self, addr: usize, value: u64, size: u32) {
        self.write_sync(addr, value, size)
    }
}

impl<R: lock_api::RawMutex + Send + Sync, T: IoMemory + Send> IoMemorySync
    for lock_api::Mutex<R, T>
{
    fn read_sync(&self, addr: usize, size: u32) -> u64 {
        self.lock().read(addr, size)
    }
    fn write_sync(&self, addr: usize, value: u64, size: u32) {
        self.lock().write(addr, value, size)
    }
}

/// A context for I/O operation.
pub trait IoContext: io::DmaContext + io::RuntimeContext {}

/// An interrupt pin.
pub trait IrqPin: Send + Sync {
    /// Set the IRQ level.
    fn set_level(&self, level: bool);

    fn raise(&self) {
        self.set_level(true);
    }
    fn lower(&self) {
        self.set_level(false);
    }
    fn pulse(&self) {
        self.raise();
        self.lower();
    }
}
