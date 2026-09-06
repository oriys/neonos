// 第 05 课：S-mode trap 的 Rust 一侧。
//
// `trap.S` 负责“CPU 刚进异常时先保存现场”，本文件负责：
// - 定义 TrapFrame 的唯一 Rust 布局；
// - 把同一组 offset 常量传给汇编；
// - 安装 stvec Direct 入口；
// - 打印一次受控异常的真实 CSR/寄存器事实。

use core::arch::{asm, global_asm};
use core::mem::{offset_of, size_of};
use core::ptr::addr_of;

// RV64 每个整数寄存器/CSR 槽位都是 8 bytes。
const SLOT_SIZE: usize = 8;

// 32 个整数寄存器 + 4 个关键 CSR。
pub const TRAP_FRAME_SIZE: usize = 36 * SLOT_SIZE;

// x0..x31 的槽位严格按照寄存器编号排列。
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

// `repr(C)` 固定字段顺序和 C-compatible alignment，避免 Rust 自行重排。
#[repr(C)]
pub struct TrapFrame {
    // x[0] 对应 x0，x[1] 对应 ra(x1)，x[2] 对应 trap 前原始 sp，以此类推。
    pub x: [usize; 32],
    pub sstatus: usize,
    pub sepc: usize,
    pub scause: usize,
    pub stval: usize,
}

// 编译期布局检查：有人修改 TrapFrame 而忘记同步汇编时，让构建尽早失败。
const _: [(); TRAP_FRAME_SIZE] = [(); size_of::<TrapFrame>()];
const _: [(); 0] = [(); offset_of!(TrapFrame, x)];
const _: [(); 32 * SLOT_SIZE] = [(); size_of::<[usize; 32]>()];
const _: [(); SSTATUS] = [(); offset_of!(TrapFrame, sstatus)];
const _: [(); SEPC] = [(); offset_of!(TrapFrame, sepc)];
const _: [(); SCAUSE] = [(); offset_of!(TrapFrame, scause)];
const _: [(); STVAL] = [(); offset_of!(TrapFrame, stval)];

// 把 Rust 侧的同一份 frame 常量注入 `trap.S`。
// 汇编不再自己维护另一套“288/offset 记忆”。
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
    // SAFETY: `trap_trigger_label` 由 `trap.S` 导出；这里只取得符号地址，不解引用。
    unsafe { addr_of!(trap_trigger_label) as usize }
}

// 第 05 课先把普通 S-mode 中断明确关着，只处理同步异常。
pub fn init() {
    let entry = trap_entry_address();

    if entry & 0b11 != 0 {
        panic!("trap_entry is not 4-byte aligned: {:#x}", entry);
    }

    // SAFETY:
    // - 当前由 OpenSBI 进入 S-mode kernel；这些 CSR 是本级允许访问的 supervisor CSR；
    // - `sie=0` 关闭具体 S interrupt enable；
    // - `csrci sstatus,2` 清 SIE 全局位；
    // - stvec 低两位为 0，选择 Direct mode。
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
    // SAFETY: 只读取刚写入的 supervisor trap-vector CSR。
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
            entry,
            actual
        );
    }

    crate::println!("trap ready");
}

// 只在第 05 课的故障注入构建中调用。
#[cfg(feature = "lesson05-illegal-trap")]
pub fn trigger_lesson05() -> ! {
    crate::println!("trigger at {:#x}", trigger_address());

    // SAFETY: 这个汇编函数专门用于受控教学故障；它的第一条指令就是非法编码。
    unsafe { trigger_illegal_instruction() };

    // 如果固件/CPU 行为意外让触发函数返回，这本身就是测试失败事实。
    panic!("lesson 05 illegal-instruction trigger unexpectedly returned");
}

fn stop() -> ! {
    loop {
        // 当前 trap 报告不可恢复；只稳定等待，不尝试 sret。
        unsafe { asm!("wfi") };
    }
}

// 汇编只在完整保存 frame 后才调用这个固定 C ABI 符号。
#[unsafe(no_mangle)]
pub extern "C" fn rust_trap_handler(frame: *const TrapFrame) -> ! {
    // SAFETY:
    // - `trap_entry` 在调用前刚在当前 kernel stack 上分配了 TRAP_FRAME_SIZE；
    // - 每个字段都已经由汇编初始化；
    // - sp 仍指向这块 frame，handler 期间不会释放它。
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

    // 本课只捕获并解释，不恢复执行；尤其不猜指令长度去 sepc += 4。
    stop()
}
