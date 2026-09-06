// 第 07 课：第一次真实进入 U-mode，并用一次 ecall 把控制权交回 S-mode。

use core::arch::{asm, global_asm};
use core::ptr::addr_of;

use crate::process::{AddressRange, Process, ProcessState, Program, UserContext, UserRuntime};
use crate::trap::{self, TrapFrame, TRAP_FRAME_SIZE};

global_asm!(include_str!("user.S"));

unsafe extern "C" {
    fn enter_user_mode(user_sp: usize, entry: usize, sstatus: usize, trap_stack_top: usize);

    static user_start: u8;
    static user_entry: u8;
    static user_ecall: u8;
    static user_end: u8;

    static user_stack_start: u8;
    static user_stack_end: u8;
    static user_trap_stack_start: u8;
    static user_trap_stack_end: u8;

    static sboot_stack: u8;
    static eboot_stack: u8;
}

const SSTATUS_SIE: usize = 1 << 1;
const SSTATUS_SPIE: usize = 1 << 5;
const SSTATUS_SPP: usize = 1 << 8;
const STACK_BYTES: usize = 16 * 1024;

fn symbol_address(symbol: *const u8) -> usize {
    symbol as usize
}

fn user_code_addresses() -> (usize, usize, usize, usize) {
    // SAFETY: 这些符号都由 `user.S` 导出；这里只取得地址。
    unsafe {
        (
            symbol_address(addr_of!(user_start)),
            symbol_address(addr_of!(user_entry)),
            symbol_address(addr_of!(user_ecall)),
            symbol_address(addr_of!(user_end)),
        )
    }
}

fn stack_ranges() -> (AddressRange, AddressRange, AddressRange) {
    // SAFETY: 这些边界符号来自 `user.S` 与 linker.ld；这里只取得地址。
    unsafe {
        (
            AddressRange::new(
                symbol_address(addr_of!(user_stack_start)),
                symbol_address(addr_of!(user_stack_end)),
            ),
            AddressRange::new(
                symbol_address(addr_of!(user_trap_stack_start)),
                symbol_address(addr_of!(user_trap_stack_end)),
            ),
            AddressRange::new(
                symbol_address(addr_of!(sboot_stack)),
                symbol_address(addr_of!(eboot_stack)),
            ),
        )
    }
}

fn trusted_user_sstatus() -> usize {
    let value: usize;

    // SAFETY: 当前代码运行在 S-mode，只读取 supervisor status CSR。
    unsafe {
        asm!(
            "csrr {value}, sstatus",
            value = out(reg) value,
            options(nomem, nostack)
        );
    }

    // SPP=0 决定 sret 返回 U-mode。
    // 本课普通 S interrupt 保持关闭，所以也清 SIE/SPIE，避免继承固件/旧路径状态。
    value & !(SSTATUS_SPP | SSTATUS_SIE | SSTATUS_SPIE)
}

fn read_sie() -> usize {
    let value: usize;
    // SAFETY: 只读取 supervisor interrupt-enable CSR。
    unsafe {
        asm!(
            "csrr {value}, sie",
            value = out(reg) value,
            options(nomem, nostack)
        );
    }
    value
}

fn read_stvec() -> usize {
    let value: usize;
    // SAFETY: 只读取 supervisor trap-vector CSR。
    unsafe {
        asm!(
            "csrr {value}, stvec",
            value = out(reg) value,
            options(nomem, nostack)
        );
    }
    value
}

fn stop() -> ! {
    loop {
        unsafe { asm!("wfi") };
    }
}

// 创建一个真实 Process 运行记录，然后把 CPU 从 S-mode 交给它。
// 这个函数成功时永远不会普通 return：用户 ecall 会进入 rust_user_trap_handler，随后停住。
pub(crate) fn run_lesson07(pid: u64) -> ! {
    let (code_start, entry, expected_ecall, code_end) = user_code_addresses();
    let program = Program::from_linked("user-ecall", entry, code_start, code_end);

    let (user_stack, trap_stack, boot_stack) = stack_ranges();

    if user_stack.size() != STACK_BYTES || trap_stack.size() != STACK_BYTES {
        panic!("lesson 07 stack size is not 16 KiB");
    }
    if user_stack.overlaps(trap_stack)
        || user_stack.overlaps(boot_stack)
        || trap_stack.overlaps(boot_stack)
    {
        panic!("lesson 07 stack regions overlap");
    }

    // 栈向低地址增长。为了让“保存的 user sp 属于 [start,end)”也能严格成立，
    // 初值使用 end-16，而不是恰好等于 one-past-end 的 end。
    let user_sp = user_stack.end - 16;
    if user_sp & 0xf != 0 || !user_stack.contains(user_sp) {
        panic!("initial user sp is not a 16-byte aligned address inside user stack");
    }
    if trap_stack.end & 0xf != 0 {
        panic!("trap stack top is not 16-byte aligned");
    }
    if entry & 0x1 != 0 {
        panic!("user entry violates current RISC-V instruction alignment");
    }
    if !program.code_range().contains(expected_ecall) {
        panic!("user ecall label is outside the linked user code range");
    }

    // `trap::init()` 已把具体 S interrupt enable 清零；进入用户前再次把它作为前置断言。
    if read_sie() != 0 {
        panic!("lesson 07 expects sie=0 before entering U-mode");
    }

    let sstatus = trusted_user_sstatus();
    if sstatus & SSTATUS_SPP != 0 {
        panic!("trusted user sstatus still has SPP=1");
    }

    let context = UserContext::new(program.entry(), user_sp, sstatus);
    let runtime = UserRuntime {
        user_stack,
        trap_stack,
        context,
    };

    let mut process = Process::new(pid, &program);
    process.attach_user_runtime(runtime);
    process.start();

    if process.state() != ProcessState::Running {
        panic!("lesson 07 process did not enter Running state");
    }

    let runtime = process.user_runtime();

    crate::println!("[user setup] pid={} program={}", process.id(), process.program().name());
    crate::println!(
        "[user setup] code=[{:#x}, {:#x})",
        process.program().code_range().start,
        process.program().code_range().end
    );
    crate::println!(
        "[user setup] user_stack=[{:#x}, {:#x}) size={}",
        runtime.user_stack.start,
        runtime.user_stack.end,
        runtime.user_stack.size()
    );
    crate::println!(
        "[user setup] trap_stack=[{:#x}, {:#x}) size={}",
        runtime.trap_stack.start,
        runtime.trap_stack.end,
        runtime.trap_stack.size()
    );
    crate::println!("[user setup] sepc={:#x}", runtime.context.sepc);
    crate::println!("[user setup] user_sp={:#x}", runtime.context.x[2]);
    crate::println!("[user setup] sscratch={:#x}", runtime.trap_stack.end);
    crate::println!("[user setup] spp=0");
    crate::println!("[user setup] process=Running");
    crate::println!("[user setup] expected_ecall={:#x}", expected_ecall);
    crate::println!("[user enter]");

    // SAFETY:
    // - 所有地址都来自当前 ELF 的受控符号并已验证；
    // - sstatus 由内核构造，SPP=0；
    // - trap stack 是内核静态 BSS，不来自用户指针；
    // - 汇编函数不会普通返回，而是执行 sret。
    unsafe {
        enter_user_mode(
            runtime.context.x[2],
            runtime.context.sepc,
            runtime.context.sstatus,
            runtime.trap_stack.end,
        );
    }

    panic!("enter_user_mode unexpectedly returned");
}

// 用户 ecall 经过 user_trap_entry 换到可信栈、保存现场、恢复 kernel gp/tp 后进入这里。
#[unsafe(no_mangle)]
pub extern "C" fn rust_user_trap_handler(frame: *const TrapFrame) -> ! {
    // SAFETY: user_trap_entry 已经完整初始化 TrapFrame，再把其起始地址作为 a0 传入。
    let frame = unsafe { &*frame };

    let (code_start, _, expected_ecall, code_end) = user_code_addresses();
    let (user_stack, trap_stack, _) = stack_ranges();

    let interrupt_bit = 1usize << (usize::BITS - 1);
    let is_interrupt = frame.scause & interrupt_bit != 0;
    let cause_code = frame.scause & !interrupt_bit;
    let origin_spp = (frame.sstatus & SSTATUS_SPP) >> 8;
    let frame_address = frame as *const TrapFrame as usize;
    let frame_end = frame_address.checked_add(TRAP_FRAME_SIZE).unwrap_or(usize::MAX);
    let frame_in_trap_stack =
        trap_stack.start <= frame_address && frame_end <= trap_stack.end;
    let user_sp_ok = user_stack.contains(frame.x[2]) && frame.x[2] & 0xf == 0;
    let sepc_in_user_code = code_start <= frame.sepc && frame.sepc < code_end;
    let kernel_stvec_restored = read_stvec() == trap::kernel_entry_address();

    crate::println!("[user trap]");
    crate::println!("origin_spp={}", origin_spp);
    crate::println!("scause={:#x}", frame.scause);
    crate::println!("cause_code={}", cause_code);
    crate::println!("sepc={:#x}", frame.sepc);
    crate::println!("expected_ecall={:#x}", expected_ecall);
    crate::println!("stval={:#x}", frame.stval);
    crate::println!("saved_user_sp={:#x}", frame.x[2]);
    crate::println!(
        "user_stack=[{:#x}, {:#x})",
        user_stack.start,
        user_stack.end
    );
    crate::println!("trap_frame={:#x}", frame_address);
    crate::println!(
        "trap_stack=[{:#x}, {:#x})",
        trap_stack.start,
        trap_stack.end
    );
    crate::println!("a0={}", frame.x[10]);
    crate::println!("a7={}", frame.x[17]);
    crate::println!("frame_in_trap_stack={}", frame_in_trap_stack);
    crate::println!("saved_user_sp_ok={}", user_sp_ok);
    crate::println!("sepc_in_user_code={}", sepc_in_user_code);
    crate::println!("ecall_match={}", frame.sepc == expected_ecall);
    crate::println!("kernel_stvec_restored={}", kernel_stvec_restored);

    let evidence_ok = origin_spp == 0
        && !is_interrupt
        && cause_code == 8
        && sepc_in_user_code
        && frame.sepc == expected_ecall
        && user_sp_ok
        && frame_in_trap_stack
        && frame.x[10] == 85
        && frame.x[17] == 1
        && kernel_stvec_restored;

    crate::println!("user_mode_evidence={}", evidence_ok);

    // 第 07 课故意不恢复用户现场、不执行 sret。
    stop()
}
