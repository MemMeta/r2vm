use riscv::{Op, Csr};
use softfp::{self, F32, F64};
use std::convert::TryInto;
use std::sync::atomic::{AtomicI32, AtomicU32, AtomicI64, AtomicU64};
use std::sync::atomic::Ordering as MemOrder;
use crate::util::AtomicMinMax;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct CacheLine {
    /// Lowest bit is used to store whether this cache line is non-writable
    /// It actually stores (tag << 1) | non-writable
    pub tag: u64,
    /// It actually stores vaddr ^ paddr
    pub paddr: u64,
}

#[repr(C)]
pub struct SharedContext {
    /// This field stored wether there are interrupts not yet seen by the current hart. Interrupts
    /// can be masked by setting SIE register or SSTATUS.SIE, yet we couldn't atomically fetch
    /// these registers and determine if the hart should be interrupted if we are not the unique
    /// owner of `Context`. The field exists to mitigate it.
    ///
    /// Our solution is to have the `new_interrupts` field. It is asserted when an external
    /// interrupt arrives, similar to SIP. This field will be periodically checked by the running
    /// hart, and the hart will stop and interrupt itself if this field is not zero, and check if
    /// it actually should take an interrupt, or the interrupt is indeed masked out. It will then
    /// reset this field to 0. By doing so, it woundn't need to check interrupts later.
    ///
    /// Timer interrupts are treated a little bit differently. To avoid to deal with atomicity
    /// issues related to `mtimecmp`, we make `mtimecmp` local to the hart. Instead, when we think
    /// there might be a new timer interrupt, we set the corresponding bit in `new_interrupts`.
    /// The hart will check and set `sip` if there is indeed a timer interrupt.
    ///
    /// Technically a bool is sufficient because we only need zero check. However we prefer to keep
    /// everything with same size and align so we will have AtomicU64 at the moment.
    pub new_interrupts: AtomicU64,

    /// Interrupts currently pending. SIP is a very special register when doing simulation,
    /// because it could be updated from outside a running hart.
    /// * When an external interrupt asserts, it should be **OR**ed with corresponding bit.
    /// * When an external interrupt deasserts, it should be **AND**ed with a mask.
    pub sip: AtomicU64,
}

impl SharedContext {
    pub fn new() -> Self {
        // Check the constant used in helper.s
        assert_eq!(offset_of!(Context, scause), 32 * 8 + 32);

        SharedContext {
            sip: AtomicU64::new(0),
            new_interrupts: AtomicU64::new(0),
        }
    }

    /// Assert interrupt using the given mask.
    pub fn assert(&self, mask: u64) {
        if self.sip.fetch_or(mask, MemOrder::Relaxed) & mask != mask {
            self.alert();
        }
    }

    /// Deassert interrupt using the given mask.
    pub fn deassert(&self, mask: u64) {
        self.sip.fetch_and(!mask, MemOrder::Relaxed);
    }

    /// Inform the hart that there might be a pending interrupt, but without actually touching
    /// `sip`. This should be called, e.g. if SIE or SSTATUS is modified.
    pub fn alert(&self) {
        self.new_interrupts.fetch_or(1, MemOrder::Release);
    }
}

/// Make #[derive(Clone)] happy for Context
impl Clone for SharedContext {
    fn clone(&self) -> Self {
        Self::new()
    }
}

/// Context representing the CPU state of a RISC-V hart.
///
/// # Memory Layout
/// As this structure is going to be accessed directly by assembly, it is important that the fields
/// are ordered well, so it can produce more optimal assembly code. We use the following order:
/// * Firstly, most commonly accessed fields are placed at the front, for example, registers,
///   counters, etc.
/// * After that, we place a shared contexts.
/// * All other items are placed at the back.
#[repr(C)]
#[derive(Clone)]
pub struct Context {
    pub registers: [u64; 32],
    pub pc: u64,
    pub instret: u64,

    pub shared: SharedContext,

    // Note that changing the position of this field would need to change the hard-fixed constant
    // in assembly.
    pub scause: u64,
    pub stval: u64,

    // Floating point states
    pub fp_registers: [u64; 32],
    pub fcsr: u64,

    // For load reservation
    pub lr_addr: u64,
    pub lr_value: u64,

    // S-mode CSRs
    pub sstatus: u64,
    pub sie: u64,
    pub stvec: u64,
    pub sscratch: u64,
    pub sepc: u64,
    pub satp: u64,

    pub timecmp: u64,

    // Current privilege level
    pub prv: u64,

    pub hartid: u64,
    pub minstret: u64,

    /// This is the L0 cache used to accelerate simulation. If a memory request hits the cache line
    /// here, then it will not go through virtual address translation nor cache simulation.
    /// Therefore this should only contain entries that are neither in the TLB nor in the cache.
    /// 
    /// The cache line should only contain valid entries for the current privilege level and ASID.
    /// Upon privilege-level switch or address space switch all entries here should be cleared.
    pub line: [CacheLine; 1024],
    pub i_line: [CacheLine; 1024],

    pub cur_block: Option<&'static DbtBlock>,
}

impl Context {
    pub fn clear_local_cache(&mut self) {
        for line in self.line.iter_mut() {
            line.tag = i64::max_value() as u64;
        }
    }

    pub fn clear_local_icache(&mut self) {
        for line in self.i_line.iter_mut() {
            line.tag = i64::max_value() as u64;
        }
    }

    pub fn test_and_set_fs(&mut self) -> Result<(), ()> {
        if self.sstatus & 0x6000 == 0 {
            self.scause = 2;
            self.stval = 0;
            return Err(())
        }
        self.sstatus |= 0x6000;
        Ok(())
    }

    /// Obtaining a bitmask of pending interrupts
    pub fn interrupt_pending(&mut self) -> u64 {
        if (self.sstatus & 0x2) != 0 { self.shared.sip.load(MemOrder::Relaxed) & self.sie } else { 0 }
    }
}

/// Perform a CSR read on a context. Note that this operation performs no checks before accessing
/// them.
/// The caller should ensure:
/// * The current privilege level has enough permission to access the CSR. CSR is nicely partition
///   into regions, so privilege check can be easily done.
/// * U-mode code does not access floating point CSRs with FS == Off.
fn read_csr(ctx: &mut Context, csr: Csr) -> Result<u64, ()> {
    if ctx.prv < csr.min_prv_level() as _ {
        ctx.scause = 2;
        ctx.stval = 0;
        return Err(())
    }
    Ok(match csr {
        Csr::Fflags => {
            ctx.test_and_set_fs()?;
            ctx.fcsr & 0b11111
        }
        Csr::Frm => {
            ctx.test_and_set_fs()?;
            (ctx.fcsr >> 5) & 0b111
        }
        Csr::Fcsr => {
            ctx.test_and_set_fs()?;
            ctx.fcsr
        }
        // Pretend that we're 100MHz
        Csr::Time => crate::event_loop().cycle() / if cfg!(feature = "fast") { 20 } else { 100 },
        // We assume the instret is incremented already
        Csr::Instret => ctx.instret - 1,
        Csr::Sstatus => {
            let mut value = ctx.sstatus;
            // SSTATUS.FS = dirty, also set SD
            if value & 0x6000 == 0x6000 { value |= 0x8000000000000000 }
            // Hard-wire UXL to 0b10, i.e. 64-bit.
            value |= 0x200000000;
            value
        }
        Csr::Sie => ctx.sie,
        Csr::Stvec => ctx.stvec,
        Csr::Scounteren => 0,
        Csr::Sscratch => ctx.sscratch,
        Csr::Sepc => ctx.sepc,
        Csr::Scause => ctx.scause,
        Csr::Stval => ctx.stval,
        Csr::Sip => ctx.shared.sip.load(MemOrder::Relaxed),
        Csr::Satp => ctx.satp,
        _ => {
            error!("read illegal csr {:x}", csr as i32);
            ctx.scause = 2;
            ctx.stval = 0;
            return Err(())
        }
    })
}

fn write_csr(ctx: &mut Context, csr: Csr, value: u64) -> Result<(), ()> {
    if csr.readonly() || ctx.prv < csr.min_prv_level() as _ {
        ctx.scause = 2;
        ctx.stval = 0;
        return Err(())
    }
    match csr {
        Csr::Fflags => {
            ctx.test_and_set_fs()?;
            ctx.fcsr = (ctx.fcsr &! 0b11111) | (value & 0b11111);
        }
        Csr::Frm => {
            ctx.test_and_set_fs()?;
            ctx.fcsr = (ctx.fcsr &! (0b111 << 5)) | ((value & 0b111) << 5);
        }
        Csr::Fcsr => {
            ctx.test_and_set_fs()?;
            ctx.fcsr = value & 0xff;
        }
        Csr::Instret => ctx.instret = value,
        Csr::Sstatus => {
            // Mask-out non-writable bits
            ctx.sstatus = value & 0xC6122;
            if ctx.interrupt_pending() !=0 { ctx.shared.alert() }
            // XXX: When MXR or SUM is changed, also clear local cache
        }
        Csr::Sie => {
            ctx.sie = value;
            if ctx.interrupt_pending() != 0 { ctx.shared.alert() }
        }
        Csr::Stvec => {
            // We support MODE 0 only at the moment
            if (value & 2) == 0 {
                ctx.stvec = value;
            }
        }
        Csr::Scounteren => (),
        Csr::Sscratch => ctx.sscratch = value,
        Csr::Sepc => ctx.sepc = value &! 1,
        Csr::Scause => ctx.scause = value,
        Csr::Stval => ctx.stval = value,
        Csr::Sip => {
            // Only SSIP flag can be cleared by software
            if value & 0x2 != 0 {
                ctx.shared.assert(2);
            } else {
                ctx.shared.deassert(2);
            }
        }
        Csr::Satp => {
            match value >> 60 {
                // No paging
                0 => ctx.satp = 0,
                // ASID not yet supported
                8 => ctx.satp = value,
                // We only support SV39 at the moment.
                _ => (),
            }
            ctx.clear_local_cache();
            ctx.clear_local_icache();
        }
        _ => {
            error!("write illegal csr {:x} = {:x}", csr as i32, value);
            ctx.scause = 2;
            ctx.stval = 0;
            return Err(())
        }
    }
    Ok(())
}

type Trap = u64;

fn translate(ctx: &mut Context, addr: u64, write: bool) -> Result<u64, Trap> {
    let fault_type = if write { 15 } else { 13 };
    if (ctx.satp >> 60) == 0 {
        return Ok(addr);
    }
    let mut ppn = ctx.satp & ((1u64 << 44) - 1);
    let mut pte: u64 = crate::emu::read_memory(ppn * 4096 + ((addr >> 30) & 511) * 8);
    if (pte & 1) == 0 { return Err(fault_type); }
    let ret = loop {
        ppn = pte >> 10;
        if (pte & 0xf) != 1 {
            break (ppn << 12) | (addr & ((1<<30)-1));
        }
        pte = crate::emu::read_memory(ppn * 4096 + ((addr >> 21) & 511) * 8);
        if (pte & 1) == 0 { return Err(fault_type); }
        ppn = pte >> 10;
        if (pte & 0xf) != 1 {
            break (ppn << 12) | (addr & ((1<<21)-1));
        }
        pte = crate::emu::read_memory(ppn * 4096 + ((addr >> 12) & 511) * 8);
        if (pte & 1) == 0 { return Err(fault_type); }

        ppn = pte >> 10;
        break (ppn << 12) | (addr & 4095);
    };
    if (pte & 0x40) == 0 || (write && ((pte & 0x4) == 0 || (pte & 0x80) == 0)) { return Err(fault_type); }
    return Ok(ret);
}

pub const CACHE_LINE_LOG2_SIZE: usize = 12;

#[inline(never)]
#[no_mangle]
fn insn_translate_cache_miss(ctx: &mut Context, addr: u64) -> Result<u64, ()> {
    let idx = addr >> CACHE_LINE_LOG2_SIZE;
    let out = match translate(ctx, addr, false) {
        Err(trap) => {
            ctx.scause = trap as u64;
            ctx.stval = addr;
            return Err(())
        }
        Ok(out) => out,
    };
    // If the cache line exists on data cache, mark it as non-writable
    // This is important as we want to capture all write to DBTed block
    let line: &mut CacheLine = &mut ctx.line[(idx & 1023) as usize];
    if (line.tag >> 1) == idx {
        line.tag |= 1;
    }
    let line: &mut CacheLine = &mut ctx.i_line[(idx & 1023) as usize];
    line.tag = idx;
    line.paddr = out ^ addr;
    Ok(out)
}

fn insn_translate(ctx: &mut Context, addr: u64) -> Result<u64, ()> {
    let idx = addr >> CACHE_LINE_LOG2_SIZE;
    let line = &ctx.i_line[(idx & 1023) as usize];
    let paddr = if line.tag != idx {
        insn_translate_cache_miss(ctx, addr)?
    } else {
        line.paddr ^ addr
    };
    Ok(paddr)
}

#[inline(never)]
#[export_name = "translate_cache_miss"]
fn translate_cache_miss(ctx: &mut Context, addr: u64, write: bool) -> Result<u64, ()> {
    let idx = addr >> CACHE_LINE_LOG2_SIZE;
    let out = match translate(ctx, addr, write) {
        Err(trap) => {
            ctx.scause = trap as u64;
            ctx.stval = addr;
            return Err(())
        }
        Ok(out) => out,
    };
    let line: &mut CacheLine = &mut ctx.line[(idx & 1023) as usize];
    line.tag = idx << 1;
    line.paddr = out ^ addr;
    if write {
        // Invalidate presence in I$, so if the code is executed, we won't silently write into it.
        let page = out >> 12 << 12;
        let start = page.saturating_sub(4096);
        let end = page + 4096;
        {
            let mut icache = icache();
            let keys: Vec<u64> = icache.range(start .. end).map(|(k,_)|*k).collect();
            for key in keys {
                icache.remove(&key);
            }
        }
        let line = &mut ctx.i_line[(idx & 1023) as usize];
        if line.tag == idx {
            line.tag = i64::max_value() as u64;
        }
    } else {
        line.tag |= 1
    }
    Ok(out)
}

fn read_vaddr<T>(ctx: &mut Context, addr: u64) -> Result<&'static T, ()> {
    ctx.minstret += 1;
    let idx = addr >> CACHE_LINE_LOG2_SIZE;
    let line = &ctx.line[(idx & 1023) as usize];
    let paddr = if (line.tag >> 1) != idx {
        translate_cache_miss(ctx, addr, false)?
    } else {
        line.paddr ^ addr
    };
    Ok(unsafe { &*(paddr as *const T) })
}

fn ptr_vaddr_x<T>(ctx: &mut Context, addr: u64) -> Result<&'static mut T, ()> {
    ctx.minstret += 1;
    let idx = addr >> CACHE_LINE_LOG2_SIZE;
    let line = &ctx.line[(idx & 1023) as usize];
    let paddr = if line.tag != (idx << 1) {
        translate_cache_miss(ctx, addr, true)?
    } else {
        line.paddr ^ addr
    };
    Ok(unsafe { &mut *(paddr as *mut T) })
}

use std::collections::BTreeMap;

#[derive(Clone, Copy)]
pub struct DbtBlock {
    /// Decoded instructions. This is pinned, as the translated code will reference its absolute location.
    pub block: &'static [(Op, bool)],
    pub code: &'static [u8],
    pub pc_map: &'static [u8],
    pub pc_start: u64,
    pub pc_end: u64,
}

#[no_mangle]
extern "C" fn handle_trap(ctx: &mut Context, pc: usize) {
    let blk = ctx.cur_block.unwrap();
    let i = crate::dbt::get_index_by_pc(&blk.pc_map, pc - blk.code.as_ptr() as usize);
    for j in i..blk.block.len() {
        ctx.pc -= if blk.block[j].1 { 2 } else { 4 };
    }
    ctx.instret -= (blk.block.len() - i) as u64;
}

/// DBT-ed instruction cache
/// ========================
///
/// It is vital that we make keep instruction cache coherent with the main memory. Alternatively we
/// can make use of the fence.i/sfence.vma instruction, but we would not like to flush the entire
/// cache when we see them because flushing the cache is very expensive, and modifying code in
/// icache is relatively rare.
/// 
/// It is very difficult to remove entries from the code cache, as there might be another hart
/// actively executing the code. To avoid messing around this scenario, we does not allow individual
/// cached blocks to be removed. Instead, we simply discard the pointer into the code cache so the
/// invalidated block will no longer be used in the future.
/// 
/// To avoid infinite growth of the cache, we will flush the cache if the amount of DBT-ed code
/// get large. This is achieved by partitioning the whole memory into two halves. Whenever we
/// cross the boundary and start allocating on the other half, we flush all pointers into the
/// code cache. The code currently executing will return after their current basic block is
/// finished, so we don't have to worry about overwriting code that is currently executing (
/// we cannot fill the entire half in a basic block's time). The allocating block will span two
/// partitions, but we don't have to worry about this, as it uses the very end of one half, so
/// next flush when crossing boundary again will invalidate it while not overwriting it.
/// 
/// Things may be a lot more complicated if we start to implement basic block chaining for extra
/// speedup. In that case we probably need some pseudo-IPI stuff to make sure nobody is executing
/// flushed or overwritten basic blocks.
const HEAP_SIZE: usize = 1024 * 1024 * 128;

lazy_static! {
    static ref ICACHE: spin::Mutex<BTreeMap<u64, &'static DbtBlock>> = {
        spin::Mutex::new(BTreeMap::default())
    };
}

fn icache() -> spin::MutexGuard<'static, BTreeMap<u64, &'static DbtBlock>> {
    ICACHE.lock()
}

lazy_static! {
    static ref CODE_CACHE: spin::Mutex<Heap> = {
        spin::Mutex::new(Heap::new())
    };
}

struct Heap(usize, usize);

impl Heap {
    fn new() -> Heap {
        let ptr = unsafe { libc::mmap(std::ptr::null_mut(), HEAP_SIZE as _, libc::PROT_READ|libc::PROT_WRITE|libc::PROT_EXEC, libc::MAP_ANONYMOUS | libc::MAP_PRIVATE, -1, 0) };
        assert_ne!(ptr, libc::MAP_FAILED);
        let ptr = ptr as usize;
        Heap(ptr, 0)
    }

    fn alloc_size(&mut self, size: usize) -> usize {
        // Crossing half-boundary
        let rollover = if self.1 <= HEAP_SIZE / 2 && self.1 + size > HEAP_SIZE / 2 {
            true
        } else if self.1 + size > HEAP_SIZE {
            // Rollover, start from zero
            self.1 = 0;
            true
        } else {
            false
        };

        if rollover {
            icache().clear()
        }

        let ret = self.1 + self.0;
        self.1 += size;
        ret
    }

    unsafe fn alloc<T: Copy>(&mut self) -> &'static mut T {
        let size = std::mem::size_of::<T>();
        &mut *(self.alloc_size(size) as *mut T)
    }

    unsafe fn alloc_slice<T: Copy>(&mut self, len: usize) -> &'static mut [T] {
        let size = std::mem::size_of::<T>();
        std::slice::from_raw_parts_mut(self.alloc_size(size * len) as *mut T, len)
    }
}

extern {
    #[allow(dead_code)]
    fn fiber_interp_block();
}

/// Broadcast sfence
fn global_sfence(mask: u64, _asid: Option<u16>, _vpn: Option<u64>) {
    unsafe {
        for i in 0..crate::CONTEXTS.len() {
            if mask & (1 << i) == 0 { continue }
            let ctx = &mut *crate::CONTEXTS[i];

            ctx.clear_local_cache();
            ctx.clear_local_icache();
        }
    }
}

fn sbi_call(ctx: &mut Context, nr: u64, arg0: u64, arg1: u64, arg2: u64, arg3: u64) -> u64 {
    match nr {
        0 => {
            ctx.timecmp = arg0 * if cfg!(feature = "fast") { 20 } else { 100 };
            ctx.shared.deassert(32);
            let shared_ctx = unsafe { &*(&ctx.shared as *const SharedContext) };
            crate::event_loop().queue(ctx.timecmp, Box::new(move || {
                shared_ctx.alert()
            }));
            0
        }
        1 => {
            crate::io::console::console_putchar(arg0 as u8);
            0
        }
        2 => crate::io::console::console_getchar() as u64,
        3 => {
            ctx.shared.deassert(2);
            0
        }
        4 => {
            let mask: u64 = crate::emu::read_memory(translate(ctx, arg0, false).unwrap());
            for i in 0..unsafe{crate::CONTEXTS.len()} {
                if mask & (1 << i) == 0 { continue }
                crate::shared_context(i).assert(2);
            }
            0
        }
        5 => {
            let mask: u64 = if arg0 == 0 {
                u64::max_value()
            } else {
                crate::emu::read_memory(translate(ctx, arg0, false).unwrap())
            };
            unsafe {
                for i in 0..crate::CONTEXTS.len() {
                    if mask & (1 << i) == 0 { continue }
                    let ctx = &mut *crate::CONTEXTS[i];
                    ctx.clear_local_icache();
                }
            }
            0
        }
        6 => {
            let mask: u64 = if arg0 == 0 {
                u64::max_value()
            } else {
                crate::emu::read_memory(translate(ctx, arg0, false).unwrap())
            };
            global_sfence(mask, None, if arg2 == 4096 { Some(arg1 >> 12) } else { None });
            0
        }
        7 => {
            let mask: u64 = if arg0 == 0 {
                u64::max_value()
            } else {
                crate::emu::read_memory(translate(ctx, arg0, false).unwrap())
            };
            global_sfence(mask, Some(arg3 as u16), if arg2 == 4096 { Some(arg1 >> 12) } else { None });
            0
        }
        8 => std::process::exit(0),
        _ => {
            panic!("unknown sbi call {}", nr);
        }
    }
}

#[export_name = "riscv_step"]
pub fn step(ctx: &mut Context, op: &Op) -> Result<(), ()> {
    macro_rules! read_reg {
        ($rs: expr) => {{
            let rs = $rs as usize;
            if rs >= 32 { unsafe { std::hint::unreachable_unchecked() } }
            ctx.registers[rs]
        }}
    }
    macro_rules! read_32 {
        ($rs: expr) => {{
            let rs = $rs as usize;
            if rs >= 32 { unsafe { std::hint::unreachable_unchecked() } }
            ctx.registers[rs] as u32
        }}
    }
    macro_rules! write_reg {
        ($rd: expr, $expression:expr) => {{
            let rd = $rd as usize;
            let value: u64 = $expression;
            if rd >= 32 { unsafe { std::hint::unreachable_unchecked() } }
            if rd != 0 { ctx.registers[rd] = value }
        }}
    }
    macro_rules! write_32 {
        ($rd: expr, $expression:expr) => {{
            let rd = $rd as usize;
            let value: u32 = $expression;
            if rd >= 32 { unsafe { std::hint::unreachable_unchecked() } }
            if rd != 0 { ctx.registers[rd] = value as i32 as u64 }
        }}
    }
    macro_rules! read_fs {
        ($rs: expr) => {{
            let rs = $rs as usize;
            if rs >= 32 { unsafe { std::hint::unreachable_unchecked() } }
            F32::new(ctx.fp_registers[rs] as u32)
        }}
    }
    macro_rules! read_fd {
        ($rs: expr) => {{
            let rs = $rs as usize;
            if rs >= 32 { unsafe { std::hint::unreachable_unchecked() } }
            F64::new(ctx.fp_registers[rs])
        }}
    }
    macro_rules! write_fs {
        ($frd: expr, $expression:expr) => {{
            let frd = $frd as usize;
            let value: F32 = $expression;
            if frd >= 32 { unsafe { std::hint::unreachable_unchecked() } }
            ctx.fp_registers[frd] = value.0 as u64 | 0xffffffff00000000
        }}
    }
    macro_rules! write_fd {
        ($frd: expr, $expression:expr) => {{
            let frd = $frd as usize;
            let value: F64 = $expression;
            if frd >= 32 { unsafe { std::hint::unreachable_unchecked() } }
            ctx.fp_registers[frd] = value.0
        }}
    }
    macro_rules! set_rm {
        ($rm: expr) => {{
            ctx.test_and_set_fs()?;
            let rm = if $rm == 0b111 { (ctx.fcsr >> 5) as u32 } else { $rm as u32 };
            let mode = match rm.try_into() {
                Ok(v) => v,
                Err(_) => trap!(2, 0),
            };
            softfp::set_rounding_mode(mode);
        }}
    }
    macro_rules! clear_flags {
        () => {
            softfp::clear_exception_flag()
        };
    }
    macro_rules! update_flags {
        () => {
            ctx.fcsr |= softfp::get_exception_flag() as u64;
        };
    }
    macro_rules! trap {
        ($cause: expr, $tval: expr) => {{
            ctx.scause = $cause;
            ctx.stval = $tval;
            return Err(())
        }}
    }

    match *op {
        Op::Illegal => { trap!(2, 0) }
        /* LOAD */
        Op::Lb { rd, rs1, imm } => {
            let vaddr = read_reg!(rs1).wrapping_add(imm as u64);
            write_reg!(rd, *read_vaddr::<u8>(ctx, vaddr)? as i8 as u64);
        }
        Op::Lh { rd, rs1, imm } => {
            let vaddr = read_reg!(rs1).wrapping_add(imm as u64);
            if vaddr & 1 != 0 { trap!(4, vaddr) }
            write_reg!(rd, *read_vaddr::<u16>(ctx, vaddr)? as i16 as u64);
        }
        Op::Lw { rd, rs1, imm } => {
            let vaddr = read_reg!(rs1).wrapping_add(imm as u64);
            if vaddr & 3 != 0 { trap!(4, vaddr) }
            write_reg!(rd, *read_vaddr::<u32>(ctx, vaddr)? as i32 as u64);
        }
        Op::Ld { rd, rs1, imm } => {
            let vaddr = read_reg!(rs1).wrapping_add(imm as u64);
            if vaddr & 7 != 0 { trap!(4, vaddr) }
            write_reg!(rd, *read_vaddr::<u64>(ctx, vaddr)?);
        }
        Op::Lbu { rd, rs1, imm } => {
            let vaddr = read_reg!(rs1).wrapping_add(imm as u64);
            write_reg!(rd, *read_vaddr::<u8>(ctx, vaddr)? as u64);
        }
        Op::Lhu { rd, rs1, imm } => {
            let vaddr = read_reg!(rs1).wrapping_add(imm as u64);
            if vaddr & 1 != 0 { trap!(4, vaddr) }
            write_reg!(rd, *read_vaddr::<u16>(ctx, vaddr)? as u64);
        }
        Op::Lwu { rd, rs1, imm } => {
            let vaddr = read_reg!(rs1).wrapping_add(imm as u64);
            if vaddr & 3 != 0 { trap!(4, vaddr) }
            write_reg!(rd, *read_vaddr::<u32>(ctx, vaddr)? as u64);
        }
        /* OP-IMM */
        Op::Addi { rd, rs1, imm } => write_reg!(rd, read_reg!(rs1).wrapping_add(imm as u64)),
        Op::Slli { rd, rs1, imm } => write_reg!(rd, read_reg!(rs1) << imm),
        Op::Slti { rd, rs1, imm } => write_reg!(rd, ((read_reg!(rs1) as i64) < (imm as i64)) as u64),
        Op::Sltiu { rd, rs1, imm } => write_reg!(rd, (read_reg!(rs1) < (imm as u64)) as u64),
        Op::Xori { rd, rs1, imm } => write_reg!(rd, read_reg!(rs1) ^ (imm as u64)),
        Op::Srli { rd, rs1, imm } => write_reg!(rd, read_reg!(rs1) >> imm),
        Op::Srai { rd, rs1, imm } => write_reg!(rd, ((read_reg!(rs1) as i64) >> imm) as u64),
        Op::Ori { rd, rs1, imm } => write_reg!(rd, read_reg!(rs1) | (imm as u64)),
        Op::Andi { rd, rs1, imm } => write_reg!(rd, read_reg!(rs1) & (imm as u64)),
        /* MISC-MEM */
        Op::Fence => std::sync::atomic::fence(MemOrder::SeqCst),
        Op::FenceI => ctx.clear_local_icache(),
        /* OP-IMM-32 */
        Op::Addiw { rd, rs1, imm } => write_reg!(rd, ((read_reg!(rs1) as i32).wrapping_add(imm)) as u64),
        Op::Slliw { rd, rs1, imm } => write_reg!(rd, ((read_reg!(rs1) as i32) << imm) as u64),
        Op::Srliw { rd, rs1, imm } => write_reg!(rd, (((read_reg!(rs1) as u32) >> imm) as i32) as u64),
        Op::Sraiw { rd, rs1, imm } => write_reg!(rd, ((read_reg!(rs1) as i32) >> imm) as u64),
        /* STORE */
        Op::Sb { rs1, rs2, imm } => {
            let vaddr = read_reg!(rs1).wrapping_add(imm as u64);
            let paddr = ptr_vaddr_x(ctx, vaddr)?;
            *paddr = read_reg!(rs2) as u8;
        }
        Op::Sh { rs1, rs2, imm } => {
            let vaddr = read_reg!(rs1).wrapping_add(imm as u64);
            if vaddr & 1 != 0 { trap!(5, vaddr) }
            let paddr = ptr_vaddr_x(ctx, vaddr)?;
            *paddr = read_reg!(rs2) as u16;
        }
        Op::Sw { rs1, rs2, imm } => {
            let vaddr = read_reg!(rs1).wrapping_add(imm as u64);
            if vaddr & 3 != 0 { trap!(5, vaddr) }
            let paddr = ptr_vaddr_x(ctx, vaddr)?;
            *paddr = read_reg!(rs2) as u32;
        }
        Op::Sd { rs1, rs2, imm } => {
            let vaddr = read_reg!(rs1).wrapping_add(imm as u64);
            if vaddr & 7 != 0 { trap!(5, vaddr) }
            let paddr = ptr_vaddr_x(ctx, vaddr)?;
            *paddr = read_reg!(rs2) as u64;
        }
        /* OP */
        Op::Add { rd, rs1, rs2 } => write_reg!(rd, read_reg!(rs1).wrapping_add(read_reg!(rs2))),
        Op::Sub { rd, rs1, rs2 } => write_reg!(rd, read_reg!(rs1).wrapping_sub(read_reg!(rs2))),
        Op::Sll { rd, rs1, rs2 } => write_reg!(rd, read_reg!(rs1) << (read_reg!(rs2) & 63)),
        Op::Slt { rd, rs1, rs2 } => write_reg!(rd, ((read_reg!(rs1) as i64) < (read_reg!(rs2) as i64)) as u64),
        Op::Sltu { rd, rs1, rs2 } => write_reg!(rd, (read_reg!(rs1) < read_reg!(rs2)) as u64),
        Op::Xor { rd, rs1, rs2 } => write_reg!(rd, read_reg!(rs1) ^ read_reg!(rs2)),
        Op::Srl { rd, rs1, rs2 } => write_reg!(rd, read_reg!(rs1) >> (read_reg!(rs2) & 63)),
        Op::Sra { rd, rs1, rs2 } => write_reg!(rd, ((read_reg!(rs1) as i64) >> (read_reg!(rs2) & 63)) as u64),
        Op::Or { rd, rs1, rs2 } => write_reg!(rd, read_reg!(rs1) | read_reg!(rs2)),
        Op::And { rd, rs1, rs2 } => write_reg!(rd, read_reg!(rs1) & read_reg!(rs2)),
        /* LUI */
        Op::Lui { rd, imm } => write_reg!(rd, imm as u64),
        Op::Addw { rd, rs1, rs2 } => write_reg!(rd, ((read_reg!(rs1) as i32).wrapping_add(read_reg!(rs2) as i32)) as u64),
        Op::Subw { rd, rs1, rs2 } => write_reg!(rd, ((read_reg!(rs1) as i32).wrapping_sub(read_reg!(rs2) as i32)) as u64),
        Op::Sllw { rd, rs1, rs2 } => write_reg!(rd, ((read_reg!(rs1) as i32) << (read_reg!(rs2) & 31)) as u64),
        Op::Srlw { rd, rs1, rs2 } => write_reg!(rd, (((read_reg!(rs1) as u32) >> (read_reg!(rs2) & 31)) as i32) as u64),
        Op::Sraw { rd, rs1, rs2 } => write_reg!(rd, ((read_reg!(rs1) as i32) >> (read_reg!(rs2) & 31)) as u64),
        /* AUIPC */
        Op::Auipc { rd, imm } => write_reg!(rd, ctx.pc.wrapping_sub(4).wrapping_add(imm as u64)),
        /* BRANCH */
        // Same as auipc, PC-relative instructions are relative to the origin pc instead of the incremented one.
        Op::Beq { rs1, rs2, imm } => {
            if read_reg!(rs1) == read_reg!(rs2) {
                ctx.pc = ctx.pc.wrapping_sub(4).wrapping_add(imm as u64);
            }
        }
        Op::Bne { rs1, rs2, imm } => {
            if read_reg!(rs1) != read_reg!(rs2) {
                ctx.pc = ctx.pc.wrapping_sub(4).wrapping_add(imm as u64);
            }
        }
        Op::Blt { rs1, rs2, imm } => {
            if (read_reg!(rs1) as i64) < (read_reg!(rs2) as i64) {
                ctx.pc = ctx.pc.wrapping_sub(4).wrapping_add(imm as u64);
            }
        }
        Op::Bge { rs1, rs2, imm } => {
            if (read_reg!(rs1) as i64) >= (read_reg!(rs2) as i64) {
                ctx.pc = ctx.pc.wrapping_sub(4).wrapping_add(imm as u64);
            }
        }
        Op::Bltu { rs1, rs2, imm } => {
            if read_reg!(rs1) < read_reg!(rs2) {
                ctx.pc = ctx.pc.wrapping_sub(4).wrapping_add(imm as u64);
            }
        }
        Op::Bgeu { rs1, rs2, imm } => {
            if read_reg!(rs1) >= read_reg!(rs2) {
                ctx.pc = ctx.pc.wrapping_sub(4).wrapping_add(imm as u64);
            }
        }
        /* JALR */
        Op::Jalr { rd, rs1, imm } => {
            let new_pc = (read_reg!(rs1).wrapping_add(imm as u64)) &! 1;
            write_reg!(rd, ctx.pc);
            ctx.pc = new_pc;
        }
        /* JAL */
        Op::Jal { rd, imm } => {
            write_reg!(rd, ctx.pc);
            ctx.pc = ctx.pc.wrapping_sub(4).wrapping_add(imm as u64);
        }
        /* SYSTEM */
        Op::Ecall =>
            if ctx.prv == 0 {
                if crate::get_flags().user_only {
                    ctx.registers[10] = unsafe { crate::emu::syscall(
                        ctx.registers[17],
                        ctx.registers[10],
                        ctx.registers[11],
                        ctx.registers[12],
                        ctx.registers[13],
                        ctx.registers[14],
                        ctx.registers[15],
                    ) };
                } else {
                    trap!(8, 0)
                }
            } else {
                ctx.registers[10] = sbi_call(
                    ctx,
                    ctx.registers[17],
                    ctx.registers[10],
                    ctx.registers[11],
                    ctx.registers[12],
                    ctx.registers[13],
                )
            }
        Op::Ebreak => trap!(3, 0),
        Op::Csrrw { rd, rs1, csr } => {
            let result = if rd != 0 { read_csr(ctx, csr)? } else { 0 };
            write_csr(ctx, csr, read_reg!(rs1))?;
            write_reg!(rd, result);
        }
        Op::Csrrs { rd, rs1, csr } => {
            let result = read_csr(ctx, csr)?;
            if rs1 != 0 { write_csr(ctx, csr, result | read_reg!(rs1))? }
            write_reg!(rd, result);
        }
        Op::Csrrc { rd, rs1, csr } => {
            let result = read_csr(ctx, csr)?;
            if rs1 != 0 { write_csr(ctx, csr, result &! read_reg!(rs1))? }
            write_reg!(rd, result);
        }
        Op::Csrrwi { rd, imm, csr } => {
            let result = if rd != 0 { read_csr(ctx, csr)? } else { 0 };
            write_csr(ctx, csr, imm as u64)?;
            write_reg!(rd, result);
        }
        Op::Csrrsi { rd, imm, csr } => {
            let result = read_csr(ctx, csr)?;
            if imm != 0 { write_csr(ctx, csr, result | imm as u64)? }
            write_reg!(rd, result);
        }
        Op::Csrrci { rd, imm, csr } => {
            let result = read_csr(ctx, csr)?;
            if imm != 0 { write_csr(ctx, csr, result &! imm as u64)? }
            write_reg!(rd, result);
        }

        /* F-extension */
        Op::Flw { frd, rs1, imm } => {
            ctx.test_and_set_fs()?;
            let vaddr = read_reg!(rs1).wrapping_add(imm as u64);
            if vaddr & 3 != 0 { trap!(4, vaddr) }
            write_fs!(frd, F32::new(*read_vaddr::<u32>(ctx, vaddr)?));
        }
        Op::Fsw { rs1, frs2, imm } => {
            ctx.test_and_set_fs()?;
            let vaddr = read_reg!(rs1).wrapping_add(imm as u64);
            if vaddr & 3 != 0 { trap!(5, vaddr) }
            let paddr = ptr_vaddr_x(ctx, vaddr)?;
            *paddr = read_fs!(frs2).0;
        }
        Op::FaddS { frd, frs1, frs2, rm } => {
            set_rm!(rm);
            clear_flags!();
            write_fs!(frd, read_fs!(frs1) + read_fs!(frs2));
            update_flags!();
        }
        Op::FsubS { frd, frs1, frs2, rm } => {
            set_rm!(rm);
            clear_flags!();
            write_fs!(frd, read_fs!(frs1) - read_fs!(frs2));
            update_flags!();
        }
        Op::FmulS { frd, frs1, frs2, rm } => {
            set_rm!(rm);
            clear_flags!();
            write_fs!(frd, read_fs!(frs1) * read_fs!(frs2));
            update_flags!();
        }
        Op::FdivS { frd, frs1, frs2, rm } => {
            set_rm!(rm);
            clear_flags!();
            write_fs!(frd, read_fs!(frs1) / read_fs!(frs2));
            update_flags!();
        }
        Op::FsqrtS { frd, frs1, rm } => {
            set_rm!(rm);
            clear_flags!();
            write_fs!(frd, read_fs!(frs1).square_root());
            update_flags!();
        }
        Op::FsgnjS { frd, frs1, frs2 } => {
            ctx.test_and_set_fs()?;
            write_fs!(frd, read_fs!(frs1).copy_sign(read_fs!(frs2)))
        }
        Op::FsgnjnS { frd, frs1, frs2 } => {
            ctx.test_and_set_fs()?;
            write_fs!(frd, read_fs!(frs1).copy_sign_negated(read_fs!(frs2)))
        }
        Op::FsgnjxS { frd, frs1, frs2 } => {
            ctx.test_and_set_fs()?;
            write_fs!(frd, read_fs!(frs1).copy_sign_xored(read_fs!(frs2)))
        }
        Op::FminS { frd, frs1, frs2 } => {
            ctx.test_and_set_fs()?;
            clear_flags!();
            write_fs!(frd, F32::min(read_fs!(frs1), read_fs!(frs2)));
            update_flags!();
        }
        Op::FmaxS { frd, frs1, frs2 } => {
            ctx.test_and_set_fs()?;
            clear_flags!();
            write_fs!(frd, F32::max(read_fs!(frs1), read_fs!(frs2)));
            update_flags!();
        }
        Op::FcvtWS { rd, frs1, rm } => {
            set_rm!(rm);
            clear_flags!();
            write_32!(rd, read_fs!(frs1).convert_to_sint::<u32>());
            update_flags!();
        }
        Op::FcvtWuS { rd, frs1, rm } => {
            set_rm!(rm);
            clear_flags!();
            write_32!(rd, read_fs!(frs1).convert_to_uint::<u32>());
            update_flags!();
        }
        Op::FcvtLS { rd, frs1, rm } => {
            set_rm!(rm);
            clear_flags!();
            write_reg!(rd, read_fs!(frs1).convert_to_sint::<u64>());
            update_flags!();
        }
        Op::FcvtLuS { rd, frs1, rm } => {
            set_rm!(rm);
            clear_flags!();
            write_reg!(rd, read_fs!(frs1).convert_to_uint::<u64>());
            update_flags!();
        }
        Op::FmvXW { rd, frs1 } => {
            ctx.test_and_set_fs()?;
            write_32!(rd, read_fs!(frs1).0);
        }
        Op::FclassS { rd, frs1 } => {
            ctx.test_and_set_fs()?;
            write_reg!(rd, 1 << read_fs!(frs1).classify() as u32);
        }
        Op::FeqS { rd, frs1, frs2 } => {
            ctx.test_and_set_fs()?;
            write_reg!(rd, (read_fs!(frs1) == read_fs!(frs2)) as u64)
        }
        Op::FltS { rd, frs1, frs2 } => {
            ctx.test_and_set_fs()?;
            clear_flags!();
            write_reg!(rd, (read_fs!(frs1) < read_fs!(frs2)) as u64);
            update_flags!();
        }
        Op::FleS { rd, frs1, frs2 } => {
            ctx.test_and_set_fs()?;
            clear_flags!();
            write_reg!(rd, (read_fs!(frs1) <= read_fs!(frs2)) as u64);
            update_flags!();
        }
        Op::FcvtSW { frd, rs1, rm } => {
            set_rm!(rm);
            clear_flags!();
            write_fs!(frd, F32::convert_from_sint::<u32>(read_32!(rs1)));
            update_flags!();
        }
        Op::FcvtSWu { frd, rs1, rm } => {
            set_rm!(rm);
            clear_flags!();
            write_fs!(frd, F32::convert_from_uint::<u32>(read_32!(rs1)));
            update_flags!();
        }
        Op::FcvtSL { frd, rs1, rm } => {
            set_rm!(rm);
            clear_flags!();
            write_fs!(frd, F32::convert_from_sint::<u64>(read_reg!(rs1)));
            update_flags!();
        }
        Op::FcvtSLu { frd, rs1, rm } => {
            set_rm!(rm);
            clear_flags!();
            write_fs!(frd, F32::convert_from_uint::<u64>(read_reg!(rs1)));
            update_flags!();
        }
        Op::FmvWX { frd, rs1 } => {
            ctx.test_and_set_fs()?;
            write_fs!(frd, F32::new(read_32!(rs1)));
        }
        Op::FmaddS { frd, frs1, frs2, frs3, rm } => {
            set_rm!(rm);
            clear_flags!();
            write_fs!(frd, F32::fused_multiply_add(read_fs!(frs1), read_fs!(frs2), read_fs!(frs3)));
            update_flags!();
        }
        Op::FmsubS { frd, frs1, frs2, frs3, rm } => {
            set_rm!(rm);
            clear_flags!();
            write_fs!(frd, F32::fused_multiply_add(read_fs!(frs1), read_fs!(frs2), -read_fs!(frs3)));
            update_flags!();
        }
        Op::FnmsubS { frd, frs1, frs2, frs3, rm } => {
            set_rm!(rm);
            clear_flags!();
            write_fs!(frd, F32::fused_multiply_add(-read_fs!(frs1), read_fs!(frs2), read_fs!(frs3)));
            update_flags!();
        }
        Op::FnmaddS { frd, frs1, frs2, frs3, rm } => {
            set_rm!(rm);
            clear_flags!();
            write_fs!(frd, -F32::fused_multiply_add(read_fs!(frs1), read_fs!(frs2), read_fs!(frs3)));
            update_flags!();
        }

        /* D-extension */
        Op::Fld { frd, rs1, imm } => {
            ctx.test_and_set_fs()?;
            let vaddr = read_reg!(rs1).wrapping_add(imm as u64);
            if vaddr & 3 != 0 { trap!(4, vaddr) }
            write_fd!(frd, F64::new(*read_vaddr::<u64>(ctx, vaddr)?));
        }
        Op::Fsd { rs1, frs2, imm } => {
            ctx.test_and_set_fs()?;
            let vaddr = read_reg!(rs1).wrapping_add(imm as u64);
            if vaddr & 7 != 0 { trap!(5, vaddr) }
            let paddr = ptr_vaddr_x(ctx, vaddr)?;
            *paddr = read_fd!(frs2).0;
        }
        Op::FaddD { frd, frs1, frs2, rm } => {
            set_rm!(rm);
            clear_flags!();
            write_fd!(frd, read_fd!(frs1) + read_fd!(frs2));
            update_flags!();
        }
        Op::FsubD { frd, frs1, frs2, rm } => {
            set_rm!(rm);
            clear_flags!();
            write_fd!(frd, read_fd!(frs1) - read_fd!(frs2));
            update_flags!();
        }
        Op::FmulD { frd, frs1, frs2, rm } => {
            set_rm!(rm);
            clear_flags!();
            write_fd!(frd, read_fd!(frs1) * read_fd!(frs2));
            update_flags!();
        }
        Op::FdivD { frd, frs1, frs2, rm } => {
            set_rm!(rm);
            clear_flags!();
            write_fd!(frd, read_fd!(frs1) / read_fd!(frs2));
            update_flags!();
        }
        Op::FsqrtD { frd, frs1, rm } => {
            set_rm!(rm);
            clear_flags!();
            write_fd!(frd, read_fd!(frs1).square_root());
            update_flags!();
        }
        Op::FsgnjD { frd, frs1, frs2 } => {
            ctx.test_and_set_fs()?;
            write_fd!(frd, read_fd!(frs1).copy_sign(read_fd!(frs2)))
        }
        Op::FsgnjnD { frd, frs1, frs2 } => {
            ctx.test_and_set_fs()?;
            write_fd!(frd, read_fd!(frs1).copy_sign_negated(read_fd!(frs2)))
        }
        Op::FsgnjxD { frd, frs1, frs2 } => {
            ctx.test_and_set_fs()?;
            write_fd!(frd, read_fd!(frs1).copy_sign_xored(read_fd!(frs2)))
        }
        Op::FminD { frd, frs1, frs2 } => {
            ctx.test_and_set_fs()?;
            clear_flags!();
            write_fd!(frd, F64::min(read_fd!(frs1), read_fd!(frs2)));
            update_flags!();
        }
        Op::FmaxD { frd, frs1, frs2 } => {
            ctx.test_and_set_fs()?;
            clear_flags!();
            write_fd!(frd, F64::max(read_fd!(frs1), read_fd!(frs2)));
            update_flags!();
        }
        Op::FcvtSD { frd, frs1, rm } => {
            set_rm!(rm);
            clear_flags!();
            write_fs!(frd, read_fd!(frs1).convert_format());
            update_flags!();
        }
        Op::FcvtDS { frd, frs1, .. } => {
            ctx.test_and_set_fs()?;
            clear_flags!();
            write_fd!(frd, read_fs!(frs1).convert_format());
            update_flags!();
        }
        Op::FcvtWD { rd, frs1, rm } => {
            set_rm!(rm);
            clear_flags!();
            write_32!(rd, read_fd!(frs1).convert_to_sint::<u32>());
            update_flags!();
        }
        Op::FcvtWuD { rd, frs1, rm } => {
            set_rm!(rm);
            clear_flags!();
            write_32!(rd, read_fd!(frs1).convert_to_uint::<u32>());
            update_flags!();
        }
        Op::FcvtLD { rd, frs1, rm } => {
            set_rm!(rm);
            clear_flags!();
            write_reg!(rd, read_fd!(frs1).convert_to_sint::<u64>());
            update_flags!();
        }
        Op::FcvtLuD { rd, frs1, rm } => {
            set_rm!(rm);
            clear_flags!();
            write_reg!(rd, read_fd!(frs1).convert_to_uint::<u64>());
            update_flags!();
        }
        Op::FmvXD { rd, frs1 } => {
            ctx.test_and_set_fs()?;
            write_reg!(rd, read_fd!(frs1).0);
        }
        Op::FclassD { rd, frs1 } => {
            ctx.test_and_set_fs()?;
            write_reg!(rd, 1 << read_fd!(frs1).classify() as u32);
        }
        Op::FeqD { rd, frs1, frs2 } => {
            ctx.test_and_set_fs()?;
            write_reg!(rd, (read_fd!(frs1) == read_fd!(frs2)) as u64)
        }
        Op::FltD { rd, frs1, frs2 } => {
            ctx.test_and_set_fs()?;
            clear_flags!();
            write_reg!(rd, (read_fd!(frs1) < read_fd!(frs2)) as u64);
            update_flags!();
        }
        Op::FleD { rd, frs1, frs2 } => {
            ctx.test_and_set_fs()?;
            clear_flags!();
            write_reg!(rd, (read_fd!(frs1) <= read_fd!(frs2)) as u64);
            update_flags!();
        }
        Op::FcvtDW { frd, rs1, rm } => {
            set_rm!(rm);
            clear_flags!();
            write_fd!(frd, F64::convert_from_sint::<u32>(read_32!(rs1)));
            update_flags!();
        }
        Op::FcvtDWu { frd, rs1, rm } => {
            set_rm!(rm);
            clear_flags!();
            write_fd!(frd, F64::convert_from_uint::<u32>(read_32!(rs1)));
            update_flags!();
        }
        Op::FcvtDL { frd, rs1, rm } => {
            set_rm!(rm);
            clear_flags!();
            write_fd!(frd, F64::convert_from_sint::<u64>(read_reg!(rs1)));
            update_flags!();
        }
        Op::FcvtDLu { frd, rs1, rm } => {
            set_rm!(rm);
            clear_flags!();
            write_fd!(frd, F64::convert_from_uint::<u64>(read_reg!(rs1)));
            update_flags!();
        }
        Op::FmvDX { frd, rs1 } => {
            ctx.test_and_set_fs()?;
            write_fd!(frd, F64::new(read_reg!(rs1)));
        }
        Op::FmaddD { frd, frs1, frs2, frs3, rm } => {
            set_rm!(rm);
            clear_flags!();
            write_fd!(frd, F64::fused_multiply_add(read_fd!(frs1), read_fd!(frs2), read_fd!(frs3)));
            update_flags!();
        }
        Op::FmsubD { frd, frs1, frs2, frs3, rm } => {
            set_rm!(rm);
            clear_flags!();
            write_fd!(frd, F64::fused_multiply_add(read_fd!(frs1), read_fd!(frs2), -read_fd!(frs3)));
            update_flags!();
        }
        Op::FnmsubD { frd, frs1, frs2, frs3, rm } => {
            set_rm!(rm);
            clear_flags!();
            write_fd!(frd, F64::fused_multiply_add(-read_fd!(frs1), read_fd!(frs2), read_fd!(frs3)));
            update_flags!();
        }
        Op::FnmaddD { frd, frs1, frs2, frs3, rm } => {
            set_rm!(rm);
            clear_flags!();
            write_fd!(frd, -F64::fused_multiply_add(read_fd!(frs1), read_fd!(frs2), read_fd!(frs3)));
            update_flags!();
        }

        /* M-extension */
        Op::Mul { rd, rs1, rs2 } => write_reg!(rd, read_reg!(rs1).wrapping_mul(read_reg!(rs2))),
        Op::Mulh { rd, rs1, rs2 } => {
            let a = read_reg!(rs1) as i64 as i128;
            let b = read_reg!(rs2) as i64 as i128;
            write_reg!(rd, ((a * b) >> 64) as u64)
        }
        Op::Mulhsu { rd, rs1, rs2 } => {
            let a = read_reg!(rs1) as i64;
            let b = read_reg!(rs2);

            // First multiply as uint128_t. This will give compiler chance to optimize better.
            let exta = a as u64 as u128;
            let extb = b as u128;
            let mut r = ((exta * extb) >> 64) as u64;

            // If rs1 < 0, then the high bits of a should be all one, but the actual bits in exta
            // is all zero. Therefore we need to compensate this error by adding multiplying
            // 0xFFFFFFFF and b, which is effective -b.
            if a < 0 { r = r.wrapping_sub(b) }
            write_reg!(rd, r)
        }
        Op::Mulhu { rd, rs1, rs2 } => {
            let a = read_reg!(rs1) as u128;
            let b = read_reg!(rs2) as u128;
            write_reg!(rd, ((a * b) >> 64) as u64)
        }
        Op::Div { rd, rs1, rs2 } => {
            let a = read_reg!(rs1) as i64;
            let b = read_reg!(rs2) as i64;
            let r = if b == 0 { -1 } else { a.wrapping_div(b) };
            write_reg!(rd, r as u64);
        }
        Op::Divu { rd, rs1, rs2 } => {
            let a = read_reg!(rs1);
            let b = read_reg!(rs2);
            let r = if b == 0 { (-1i64) as u64 } else { a / b };
            write_reg!(rd, r);
        }
        Op::Rem { rd, rs1, rs2 } => {
            let a = read_reg!(rs1) as i64;
            let b = read_reg!(rs2) as i64;
            let r = if b == 0 { a } else { a.wrapping_rem(b) };
            write_reg!(rd, r as u64);
        }
        Op::Remu { rd, rs1, rs2 } => {
            let a = read_reg!(rs1);
            let b = read_reg!(rs2);
            let r = if b == 0 { a } else { a % b };
            write_reg!(rd, r);
        }
        Op::Mulw { rd, rs1, rs2 } => write_reg!(rd, ((read_reg!(rs1) as i32).wrapping_mul(read_reg!(rs2) as i32)) as u64),
        Op::Divw { rd, rs1, rs2 } => {
            let a = read_reg!(rs1) as i32;
            let b = read_reg!(rs2) as i32;
            let r = if b == 0 { -1 } else { a.wrapping_div(b) };
            write_reg!(rd, r as u64);
        }
        Op::Divuw { rd, rs1, rs2 } => {
            let a = read_reg!(rs1) as u32;
            let b = read_reg!(rs2) as u32;
            let r = if b == 0 { (-1i32) as u32 } else { a / b };
            write_reg!(rd, r as i32 as u64);
        }
        Op::Remw { rd, rs1, rs2 } => {
            let a = read_reg!(rs1) as i32;
            let b = read_reg!(rs2) as i32;
            let r = if b == 0 { a } else { a.wrapping_rem(b) };
            write_reg!(rd, r as u64);
        }
        Op::Remuw { rd, rs1, rs2 } => {
            let a = read_reg!(rs1) as u32;
            let b = read_reg!(rs2) as u32;
            let r = if b == 0 { a } else { a % b };
            write_reg!(rd, r as i32 as u64);
        }

        /* A-extension */
        Op::LrW { rd, rs1 } => {
            let addr = read_reg!(rs1);
            if addr & 3 != 0 { trap!(5, addr) }
            let ptr = ptr_vaddr_x::<AtomicU32>(ctx, addr)?;
            let value = ptr.load(MemOrder::SeqCst) as i32 as u64;
            write_reg!(rd, value);
            ctx.lr_addr = addr;
            ctx.lr_value = value;
        }
        Op::LrD { rd, rs1 } => {
            let addr = read_reg!(rs1);
            if addr & 7 != 0 { trap!(5, addr) }
            let ptr = ptr_vaddr_x::<AtomicU64>(ctx, addr)?;
            let value = ptr.load(MemOrder::SeqCst);
            write_reg!(rd, value);
            ctx.lr_addr = addr;
            ctx.lr_value = value;
        }
        Op::ScW { rd, rs1, rs2 } => {
            let addr = read_reg!(rs1);
            if addr & 3 != 0 { trap!(5, addr) }
            let src = read_reg!(rs2) as u32;
            let result = if addr != ctx.lr_addr {
                1
            } else {
                let ptr = ptr_vaddr_x::<AtomicU32>(ctx, addr)?;
                match ptr.compare_exchange(ctx.lr_value as u32, src, MemOrder::SeqCst, MemOrder::SeqCst) {
                    Ok(_) => 0,
                    Err(_) => 1,
                }
            };
            write_reg!(rd, result);
        }
        Op::ScD { rd, rs1, rs2 } => {
            let addr = read_reg!(rs1);
            if addr & 7 != 0 { trap!(5, addr) }
            let src = read_reg!(rs2);
            let result = if addr != ctx.lr_addr {
                1
            } else {
                let ptr = ptr_vaddr_x::<AtomicU64>(ctx, addr)?;
                match ptr.compare_exchange(ctx.lr_value, src, MemOrder::SeqCst, MemOrder::SeqCst) {
                    Ok(_) => 0,
                    Err(_) => 1,
                }
            };
            write_reg!(rd, result)
        }
        Op::AmoswapW { rd, rs1, rs2 } => {
            let addr = read_reg!(rs1);
            if addr & 3 != 0 { trap!(5, addr) }
            let src = read_reg!(rs2) as u32;
            let ptr = ptr_vaddr_x::<AtomicU32>(ctx, addr)?;
            let current = ptr.swap(src, MemOrder::SeqCst);
            write_32!(rd, current);
        }
        Op::AmoswapD { rd, rs1, rs2 } => {
            let addr = read_reg!(rs1);
            if addr & 7 != 0 { trap!(5, addr) }
            let src = read_reg!(rs2);
            let ptr = ptr_vaddr_x::<AtomicU64>(ctx, addr)?;
            let current = ptr.swap(src, MemOrder::SeqCst);
            write_reg!(rd, current);
        }
        Op::AmoaddW { rd, rs1, rs2 } => {
            let addr = read_reg!(rs1);
            if addr & 3 != 0 { trap!(5, addr) }
            let src = read_reg!(rs2) as u32;
            let ptr = ptr_vaddr_x::<AtomicU32>(ctx, addr)?;
            let current = ptr.fetch_add(src, MemOrder::SeqCst);
            write_32!(rd, current);
        }
        Op::AmoaddD { rd, rs1, rs2 } => {
            let addr = read_reg!(rs1);
            if addr & 7 != 0 { trap!(5, addr) }
            let src = read_reg!(rs2);
            let ptr = ptr_vaddr_x::<AtomicU64>(ctx, addr)?;
            let current = ptr.fetch_add(src, MemOrder::SeqCst);
            write_reg!(rd, current);
        }
        Op::AmoandW { rd, rs1, rs2 } => {
            let addr = read_reg!(rs1);
            if addr & 3 != 0 { trap!(5, addr) }
            let src = read_reg!(rs2) as u32;
            let ptr = ptr_vaddr_x::<AtomicU32>(ctx, addr)?;
            let current = ptr.fetch_and(src, MemOrder::SeqCst);
            write_32!(rd, current);
        }
        Op::AmoandD { rd, rs1, rs2 } => {
            let addr = read_reg!(rs1);
            if addr & 7 != 0 { trap!(5, addr) }
            let src = read_reg!(rs2);
            let ptr = ptr_vaddr_x::<AtomicU64>(ctx, addr)?;
            let current = ptr.fetch_and(src, MemOrder::SeqCst);
            write_reg!(rd, current);
        }
        Op::AmoorW { rd, rs1, rs2 } => {
            let addr = read_reg!(rs1);
            if addr & 3 != 0 { trap!(5, addr) }
            let src = read_reg!(rs2) as u32;
            let ptr = ptr_vaddr_x::<AtomicU32>(ctx, addr)?;
            let current = ptr.fetch_or(src, MemOrder::SeqCst);
            write_32!(rd, current);
        }
        Op::AmoorD { rd, rs1, rs2 } => {
            let addr = read_reg!(rs1);
            if addr & 7 != 0 { trap!(5, addr) }
            let src = read_reg!(rs2);
            let ptr = ptr_vaddr_x::<AtomicU64>(ctx, addr)?;
            let current = ptr.fetch_or(src, MemOrder::SeqCst);
            write_reg!(rd, current);
        }
        Op::AmoxorW { rd, rs1, rs2 } => {
            let addr = read_reg!(rs1);
            if addr & 3 != 0 { trap!(5, addr) }
            let src = read_reg!(rs2) as u32;
            let ptr = ptr_vaddr_x::<AtomicU32>(ctx, addr)?;
            let current = ptr.fetch_xor(src, MemOrder::SeqCst);
            write_32!(rd, current);
        }
        Op::AmoxorD { rd, rs1, rs2 } => {
            let addr = read_reg!(rs1);
            if addr & 7 != 0 { trap!(5, addr) }
            let src = read_reg!(rs2);
            let ptr = ptr_vaddr_x::<AtomicU64>(ctx, addr)?;
            let current = ptr.fetch_xor(src, MemOrder::SeqCst);
            write_reg!(rd, current);
        }
        Op::AmominW { rd, rs1, rs2 } => {
            let addr = read_reg!(rs1);
            if addr & 3 != 0 { trap!(5, addr) }
            let src = read_reg!(rs2) as u32;
            let ptr = ptr_vaddr_x::<AtomicI32>(ctx, addr)?;
            let current = ptr.fetch_min_stable(src as i32, MemOrder::SeqCst);
            write_32!(rd, current as u32);
        }
        Op::AmominD { rd, rs1, rs2 } => {
            let addr = read_reg!(rs1);
            if addr & 7 != 0 { trap!(5, addr) }
            let src = read_reg!(rs2);
            let ptr = ptr_vaddr_x::<AtomicI64>(ctx, addr)?;
            let current = ptr.fetch_min_stable(src as i64, MemOrder::SeqCst);
            write_reg!(rd, current as u64);
        }
        Op::AmomaxW { rd, rs1, rs2 } => {
            let addr = read_reg!(rs1);
            if addr & 3 != 0 { trap!(5, addr) }
            let src = read_reg!(rs2) as u32;
            let ptr = ptr_vaddr_x::<AtomicI32>(ctx, addr)?;
            let current = ptr.fetch_max_stable(src as i32, MemOrder::SeqCst);
            write_32!(rd, current as u32);
        }
        Op::AmomaxD { rd, rs1, rs2 } => {
            let addr = read_reg!(rs1);
            if addr & 7 != 0 { trap!(5, addr) }
            let src = read_reg!(rs2);
            let ptr = ptr_vaddr_x::<AtomicI64>(ctx, addr)?;
            let current = ptr.fetch_max_stable(src as i64, MemOrder::SeqCst);
            write_reg!(rd, current as u64);
        }
        Op::AmominuW { rd, rs1, rs2 } => {
            let addr = read_reg!(rs1);
            if addr & 3 != 0 { trap!(5, addr) }
            let src = read_reg!(rs2) as u32;
            let ptr = ptr_vaddr_x::<AtomicU32>(ctx, addr)?;
            let current = ptr.fetch_min_stable(src, MemOrder::SeqCst);
            write_32!(rd, current);
        }
        Op::AmominuD { rd, rs1, rs2 } => {
            let addr = read_reg!(rs1);
            if addr & 7 != 0 { trap!(5, addr) }
            let src = read_reg!(rs2);
            let ptr = ptr_vaddr_x::<AtomicU64>(ctx, addr)?;
            let current = ptr.fetch_min_stable(src, MemOrder::SeqCst);
            write_reg!(rd, current);
        }
        Op::AmomaxuW { rd, rs1, rs2 } => {
            let addr = read_reg!(rs1);
            if addr & 3 != 0 { trap!(5, addr) }
            let src = read_reg!(rs2) as u32;
            let ptr = ptr_vaddr_x::<AtomicU32>(ctx, addr)?;
            let current = ptr.fetch_max_stable(src, MemOrder::SeqCst);
            write_32!(rd, current);
        }
        Op::AmomaxuD { rd, rs1, rs2 } => {
            let addr = read_reg!(rs1);
            if addr & 7 != 0 { trap!(5, addr) }
            let src = read_reg!(rs2);
            let ptr = ptr_vaddr_x::<AtomicU64>(ctx, addr)?;
            let current = ptr.fetch_max_stable(src, MemOrder::SeqCst);
            write_reg!(rd, current);
        }

        /* Privileged */
        Op::Sret => {
            if ctx.prv != 1 { trap!(2, 0) }
            ctx.pc = ctx.sepc;

            // Set privilege according to SPP
            if (ctx.sstatus & 0x100) != 0 {
                ctx.prv = 1;
            } else {
                ctx.prv = 0;
                // Switch from S-mode to U-mode, clear local cache
                ctx.clear_local_cache();
                ctx.clear_local_icache();
            }

            // Set SIE according to SPIE
            if (ctx.sstatus & 0x20) != 0 {
                ctx.sstatus |= 0x2;
            } else {
                ctx.sstatus &=! 0x2;
            }

            // Set SPIE to 1
            ctx.sstatus |= 0x20;
            // Set SPP to U
            ctx.sstatus &=! 0x100;
        }
        Op::Wfi => {
            if ctx.prv != 1 { trap!(2, 0) }
        }
        Op::SfenceVma { rs1, rs2 } => {
            if ctx.prv != 1 { trap!(2, 0) }
            let asid = if rs2 == 0 { None } else { Some(read_reg!(rs2) as u16) };
            let vpn = if rs1 == 0 { None } else { Some(read_reg!(rs1) >> 12) };
            global_sfence(1 << ctx.hartid, asid, vpn)
        }
    }
    Ok(())
}

extern "C" fn no_op() {}

#[no_mangle]
extern "C" fn interp_block(ctx: &mut Context) {
    let dbtblk = ctx.cur_block.unwrap();
    ctx.instret += dbtblk.block.len() as u64;

    for i in 0..dbtblk.block.len() {
        if i != 0 { crate::fiber::Fiber::sleep(1) }

        // The instruction is on a new cache line, force an access to I$
        let cache_line_size = 1 << CACHE_LINE_LOG2_SIZE;
        if ctx.pc & (cache_line_size - 1) == 0 || ctx.pc & (cache_line_size - 1) == cache_line_size - 2 {
            insn_translate(ctx, ctx.pc).unwrap();
        }

        let (ref inst, compressed) = dbtblk.block[i];
        ctx.pc += if compressed { 2 } else { 4 };
        match step(ctx, inst) {
            Ok(()) => (),
            Err(()) => {
                ctx.pc = ctx.pc - if compressed { 2 } else { 4 };
                ctx.instret -= (dbtblk.block.len() - i) as u64;
                trap(ctx);
                return;
            }
        }
    }
}

fn decode_instr(pc: &mut u64, pc_next: u64) -> (Op, bool) {
    let bits = crate::emu::read_memory::<u16>(*pc);
    if bits & 3 == 3 {
        let hi_bits = if *pc & 4095 == 4094 {
            crate::emu::read_memory::<u16>(pc_next)
        } else {
            crate::emu::read_memory::<u16>(*pc + 2)
        };
        let bits = (hi_bits as u32) << 16 | bits as u32;
        let (op, c) = (riscv::decode::decode(bits), false);
        if crate::get_flags().disassemble {
            riscv::disasm::print_instr(*pc, bits, &op);
        }
        *pc += 4;
        (op, c)
    } else {
        let (op, c) = (riscv::decode::decode_compressed(bits), true);
        if crate::get_flags().disassemble {
            riscv::disasm::print_instr(*pc, bits as u32, &op);
        }
        *pc += 2;
        (op, c)
    }
}

fn decode_block(mut pc: u64, pc_next: u64) -> (Vec<(Op, bool)>, u64, u64) {
    let start_pc = pc;
    let mut vec = Vec::new();

    if crate::get_flags().disassemble {
        eprintln!("Decoding {:x}", pc);
    }

    loop {
        let (op, c) = decode_instr(&mut pc, pc_next);
        if op.can_change_control_flow() || (pc &! 4095) != (start_pc &! 4095) {
            vec.push((op, c));
            break
        }
        vec.push((op, c));
    }
    (vec, start_pc, pc)
}


#[no_mangle]
fn find_block(ctx: &mut Context) -> unsafe extern "C" fn() {
    let pc = ctx.pc;
    let phys_pc = match insn_translate(ctx, pc) {
        Ok(pc) => pc,
        Err(_) => {
            trap(ctx);
            return no_op
        }
    };
    let dbtblk: &DbtBlock = match { let icache = icache(); icache.get(&phys_pc).map(|x|*x) } {
        Some(v) => v,
        None => {
            // Ignore error in this case
            let phys_pc_next = match translate(ctx, (pc &! 4095) + 4096, false) {
                Ok(pc) => pc,
                Err(_) => 0,
            };

            let (vec, start, end) = decode_block(phys_pc, phys_pc_next);
            let mut code_cache = CODE_CACHE.lock();
            let op_slice = unsafe { code_cache.alloc_slice(vec.len()) };
            op_slice.copy_from_slice(&vec);

            let mut compiler = crate::dbt::DbtCompiler::new();
            compiler.compile((&op_slice, start, end));
            
            let code = unsafe { code_cache.alloc_slice(compiler.enc.buffer.len()) };
            code.copy_from_slice(&compiler.enc.buffer);

            let map = unsafe { code_cache.alloc_slice(compiler.pc_map.len()) };
            map.copy_from_slice(&compiler.pc_map);

            let block = unsafe { code_cache.alloc() };
            *block = DbtBlock {
                block: op_slice,
                code: code,
                pc_map: map,
                pc_start: start,
                pc_end: end,
            };
            icache().insert(phys_pc, block);
            block
        }
    };

    ctx.cur_block = Some(dbtblk);
    unsafe { std::mem::transmute(dbtblk.code.as_ptr() as usize) }
}

#[no_mangle]
/// Check if an enabled interrupt is pending, and take it if so.
pub fn check_interrupt(ctx: &mut Context) {
    let _ = ctx.shared.new_interrupts.swap(0, MemOrder::Acquire);

    if crate::event_loop().cycle() >= ctx.timecmp {
        ctx.shared.sip.fetch_or(32, MemOrder::Relaxed);
    }

    // Find out which interrupts can be taken
    let interrupt_mask = ctx.interrupt_pending();
    // No interrupt pending
    if interrupt_mask == 0 { return }
    // Find the highest priority interrupt
    let pending = 63 - interrupt_mask.leading_zeros() as u64;
    // Interrupts have the highest bit set
    // TODO: Reconsider atomicity
    ctx.scause = (1 << 63) | pending;
    ctx.stval = 0;
    trap(ctx);
}

/// Trigger a trap. pc must be already adjusted properly before calling.
#[no_mangle]
pub fn trap(ctx: &mut Context) {
    if crate::get_flags().user_only {
        eprintln!("unhandled trap {:x}, tval = {:x}", ctx.scause, ctx.stval);
        eprintln!("pc  = {:16x}  ra  = {:16x}", ctx.pc, ctx.registers[1]);
        for i in (2..32).step_by(2) {
            eprintln!(
                "{:-3} = {:16x}  {:-3} = {:16x}",
                riscv::disasm::REG_NAMES[i], ctx.registers[i],
                riscv::disasm::REG_NAMES[i + 1], ctx.registers[i + 1]
            );
        }
        std::process::exit(1);
    }

    ctx.sepc = ctx.pc;

    // Clear or set SPP bit
    if ctx.prv != 0 {
        ctx.sstatus |= 0x100;
    } else {
        ctx.sstatus &=! 0x100;
        // Switch from U-mode to S-mode, clear local cache
        ctx.clear_local_cache();
        ctx.clear_local_icache();
    }
    // Clear of set SPIE bit
    if (ctx.sstatus & 0x2) != 0 {
        ctx.sstatus |= 0x20;
    } else {
        ctx.sstatus &=! 0x20;
    }
    // Clear SIE
    ctx.sstatus &= !0x2;
    // Switch to S-mode
    ctx.prv = 1;
    ctx.pc = ctx.stvec;
}
