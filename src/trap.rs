// 第 05～09 课的 trap / context 布局定义。
//
// TrapFrame：用户/内核被 trap 时的完整整数现场。
// KernelContext：第 09 课 `run_user` 汇编桥为了“以后正常返回原 Rust 调用者”保存的内核调用现场。

use core::arch::{asm, global_asm};
use core::mem::{offset_of, size_of};
use core::ptr::addr_of;

const SLOT_SIZE: usize = 8;

// ---------- TrapFrame ----------
pub const TRAP_FRAME_SIZE: usize = 36 * SLOT_SIZE;

const X0: usize = 0 * SLOT_SIZE;
const X1: usize = 1 * SLOT_SIZE;
const X2: usize = 2 * SLOT_SIZE;
const X3: usize = 3 * SLOT_SIZE;
const X4: usize = 4 * SLOT_SIZE;
const X5: usize = 5 * SLOT_SIZE;
const X6: usize = 6 * SLOT_SIZE;
const X7: usize = 7 * SLOT_SIZE;
const X8: usize = 8 * SLOT_SIZE;
const X9: usize = 9 * SLOT_SIZE;
const X10: usize = 10 * SLOT_SIZE;
const X11: usize = 11 * SLOT_SIZE;
const X12: usize = 12 * SLOT_SIZE;
const X13: usize = 13 * SLOT_SIZE;
const X14: usize = 14 * SLOT_SIZE;
const X15: usize = 15 * SLOT_SIZE;
const X16: usize = 16 * SLOT_SIZE;
const X17: usize = 17 * SLOT_SIZE;
const X18: usize = 18 * SLOT_SIZE;
const X19: usize = 19 * SLOT_SIZE;
const X20: usize = 20 * SLOT_SIZE;
const X21: usize = 21 * SLOT_SIZE;
const X22: usize = 22 * SLOT_SIZE;
const X23: usize = 23 * SLOT_SIZE;
const X24: usize = 24 * SLOT_SIZE;
const X25: usize = 25 * SLOT_SIZE;
const X26: usize = 26 * SLOT_SIZE;
const X27: usize = 27 * SLOT_SIZE;
const X28: usize = 28 * SLOT_SIZE;
const X29: usize = 29 * SLOT_SIZE;
const X30: usize = 30 * SLOT_SIZE;
const X31: usize = 31 * SLOT_SIZE;

const SSTATUS: usize = 32 * SLOT_SIZE;
const SEPC: usize = 33 * SLOT_SIZE;
const SCAUSE: usize = 34 * SLOT_SIZE;
const STVAL: usize = 35 * SLOT_SIZE;

#[derive(Clone, Copy)]
#[repr(C)]
pub struct TrapFrame {
    pub x: [usize; 32],
    pub sstatus: usize,
    pub sepc: usize,
    pub scause: usize,
    pub stval: usize,
}

const _: [(); TRAP_FRAME_SIZE] = [(); size_of::<TrapFrame>()];
const _: [(); 0] = [(); offset_of!(TrapFrame, x)];
const _: [(); 32 * SLOT_SIZE] = [(); size_of::<[usize; 32]>()];
const _: [(); SSTATUS] = [(); offset_of!(TrapFrame, sstatus)];
const _: [(); SEPC] = [(); offset_of!(TrapFrame, sepc)];
const _: [(); SCAUSE] = [(); offset_of!(TrapFrame, scause)];
const _: [(); STVAL] = [(); offset_of!(TrapFrame, stval)];

// ---------- KernelContext ----------
// `run_user` 是一个普通 C ABI callee，所以要保证恢复后原 Rust caller 看到的 callee-saved 状态没有变化。
// 除 ra/sp/s0..s11 外，本课程还显式保存 gp/tp，避免用户代码修改后污染可信内核运行环境。
#[repr(C)]
pub struct KernelContext {
    pub ra: usize,
    pub sp: usize,
    pub gp: usize,
    pub tp: usize,
    pub s: [usize; 12],
}

pub const KERNEL_CONTEXT_SIZE: usize = size_of::<KernelContext>();
pub const KCTX_RA: usize = offset_of!(KernelContext, ra);
pub const KCTX_SP: usize = offset_of!(KernelContext, sp);
pub const KCTX_GP: usize = offset_of!(KernelContext, gp);
pub const KCTX_TP: usize = offset_of!(KernelContext, tp);
pub const KCTX_S0: usize = offset_of!(KernelContext, s) + 0 * SLOT_SIZE;
pub const KCTX_S1: usize = offset_of!(KernelContext, s) + 1 * SLOT_SIZE;
pub const KCTX_S2: usize = offset_of!(KernelContext, s) + 2 * SLOT_SIZE;
pub const KCTX_S3: usize = offset_of!(KernelContext, s) + 3 * SLOT_SIZE;
pub const KCTX_S4: usize = offset_of!(KernelContext, s) + 4 * SLOT_SIZE;
pub const KCTX_S5: usize = offset_of!(KernelContext, s) + 5 * SLOT_SIZE;
pub const KCTX_S6: usize = offset_of!(KernelContext, s) + 6 * SLOT_SIZE;
pub const KCTX_S7: usize = offset_of!(KernelContext, s) + 7 * SLOT_SIZE;
pub const KCTX_S8: usize = offset_of!(KernelContext, s) + 8 * SLOT_SIZE;
pub const KCTX_S9: usize = offset_of!(KernelContext, s) + 9 * SLOT_SIZE;
pub const KCTX_S10: usize = offset_of!(KernelContext, s) + 10 * SLOT_SIZE;
pub const KCTX_S11: usize = offset_of!(KernelContext, s) + 11 * SLOT_SIZE;

const _: [(); 128] = [(); KERNEL_CONTEXT_SIZE];
const _: [(); 0] = [(); KCTX_RA];
const _: [(); 8] = [(); KCTX_SP];
const _: [(); 16] = [(); KCTX_GP];
const _: [(); 24] = [(); KCTX_TP];
const _: [(); 32] = [(); KCTX_S0];
const _: [(); 120] = [(); KCTX_S11];

// trap.S 同时需要 TrapFrame 和 KernelContext 的唯一布局常量。
global_asm!(
    include_str!("trap.S"),
    tf_size = const TRAP_FRAME_SIZE,
    x0 = const X0,
    x1 = const X1,
    x2 = const X2,
    x3 = const X3,
    x4 = const X4,
    x5 = const X5,
    x6 = const X6,
    x7 = const X7,
    x8 = const X8,
    x9 = const X9,
    x10 = const X10,
    x11 = const X11,
    x12 = const X12,
    x13 = const X13,
    x14 = const X14,
    x15 = const X15,
    x16 = const X16,
    x17 = const X17,
    x18 = const X18,
    x19 = const X19,
    x20 = const X20,
    x21 = const X21,
    x22 = const X22,
    x23 = const X23,
    x24 = const X24,
    x25 = const X25,
    x26 = const X26,
    x27 = const X27,
    x28 = const X28,
    x29 = const X29,
    x30 = const X30,
    x31 = const X31,
    sstatus = const SSTATUS,
    sepc = const SEPC,
    scause = const SCAUSE,
    stval = const STVAL,
    kctx_size = const KERNEL_CONTEXT_SIZE,
    kctx_ra = const KCTX_RA,
    kctx_sp = const KCTX_SP,
    kctx_gp = const KCTX_GP,
    kctx_tp = const KCTX_TP,
    kctx_s0 = const KCTX_S0,
    kctx_s1 = const KCTX_S1,
    kctx_s2 = const KCTX_S2,
    kctx_s3 = const KCTX_S3,
    kctx_s4 = const KCTX_S4,
    kctx_s5 = const KCTX_S5,
    kctx_s6 = const KCTX_S6,
    kctx_s7 = const KCTX_S7,
    kctx_s8 = const KCTX_S8,
    kctx_s9 = const KCTX_S9,
    kctx_s10 = const KCTX_S10,
    kctx_s11 = const KCTX_S11,
);

unsafe extern "C" {
    fn trap_entry();
    fn trigger_illegal_instruction();
    static trap_trigger_label: u8;
}

fn trap_entry_address() -> usize {
    trap_entry as *const () as usize
}

pub fn trigger_address() -> usize {
    unsafe { addr_of!(trap_trigger_label) as usize }
}

pub fn init() {
    let entry = trap_entry_address();

    if entry & 0b11 != 0 {
        panic!("trap_entry is not 4-byte aligned: {:#x}", entry);
    }

    unsafe {
        asm!(
            "csrw sie, zero",
            "csrci sstatus, 2",
            "csrw stvec, {entry}",
            entry = in(reg) entry,
            options(nomem, nostack)
        );
    }

    let actual: usize;
    unsafe {
        asm!(
            "csrr {actual}, stvec",
            actual = out(reg) actual,
            options(nomem, nostack)
        );
    }

    if actual != entry {
        panic!(
            "stvec readback mismatch: expected={:#x} actual={:#x}",
            entry, actual
        );
    }

    crate::println!("trap ready");
}

#[cfg(feature = "lesson05-illegal-trap")]
pub fn trigger_lesson05() -> ! {
    crate::println!("trigger at {:#x}", trigger_address());
    unsafe { trigger_illegal_instruction() };
    panic!("lesson 05 illegal-instruction trigger unexpectedly returned");
}

fn stop() -> ! {
    loop {
        unsafe { asm!("wfi") };
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn rust_trap_handler(frame: *const TrapFrame) -> ! {
    let frame = unsafe { &*frame };

    let interrupt_bit = 1usize << (usize::BITS - 1);
    let is_interrupt = frame.scause & interrupt_bit != 0;
    let cause_code = frame.scause & !interrupt_bit;
    let expected_trigger = trigger_address();

    crate::println!("[trap]");
    crate::println!("scause={:#x}", frame.scause);
    crate::println!("sepc={:#x}", frame.sepc);
    crate::println!("stval={:#x}", frame.stval);
    crate::println!("sstatus={:#x}", frame.sstatus);
    crate::println!("original_sp={:#x}", frame.x[2]);
    crate::println!("interrupt={}", is_interrupt);
    crate::println!("cause_code={}", cause_code);
    crate::println!("trigger={:#x}", expected_trigger);
    crate::println!("trigger_match={}", frame.sepc == expected_trigger);

    stop()
}
