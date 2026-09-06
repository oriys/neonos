// 第 07～08 课：真实 U-mode 运行、用户 trap 证据，以及第一次 syscall 返回。

use core::arch::{asm, global_asm};
use core::ptr::addr_of;

use crate::process::{AddressRange, Process, ProcessState, Program, UserContext, UserRuntime};
use crate::trap::{TrapFrame, TRAP_FRAME_SIZE};

global_asm!(include_str!("user.S"));

#[cfg(all(feature = "lesson07-user-mode", feature = "lesson08-syscalls"))]
compile_error!("lesson07-user-mode and lesson08-syscalls are mutually exclusive checkpoints");

unsafe extern "C" {
    fn enter_user_mode(user_sp: usize, entry: usize, sstatus: usize, trap_stack_top: usize);
    fn trap_entry();

    // 第 07 课用户程序。
    static user_start: u8;
    static user_entry: u8;
    static user_ecall: u8;
    static user_end: u8;

    // 第 08 课用户程序与所有预期 ecall 位置。
    static user08_start: u8;
    static user08_entry: u8;
    static user08_ecall_unknown: u8;
    static user08_ecall_invalid: u8;
    static user08_ecall_o: u8;
    static user08_ecall_k: u8;
    static user08_ecall_newline: u8;
    static user08_end: u8;

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

fn kernel_trap_entry_address() -> usize {
    trap_entry as *const () as usize
}

fn user07_code_addresses() -> (usize, usize, usize, usize) {
    unsafe {
        (
            symbol_address(addr_of!(user_start)),
            symbol_address(addr_of!(user_entry)),
            symbol_address(addr_of!(user_ecall)),
            symbol_address(addr_of!(user_end)),
        )
    }
}

fn user08_code_addresses() -> (usize, usize, usize) {
    unsafe {
        (
            symbol_address(addr_of!(user08_start)),
            symbol_address(addr_of!(user08_entry)),
            symbol_address(addr_of!(user08_end)),
        )
    }
}

fn user08_known_ecall(address: usize) -> bool {
    unsafe {
        address == symbol_address(addr_of!(user08_ecall_unknown))
            || address == symbol_address(addr_of!(user08_ecall_invalid))
            || address == symbol_address(addr_of!(user08_ecall_o))
            || address == symbol_address(addr_of!(user08_ecall_k))
            || address == symbol_address(addr_of!(user08_ecall_newline))
    }
}

fn stack_ranges() -> (AddressRange, AddressRange, AddressRange) {
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

    unsafe {
        asm!(
            "csrr {value}, sstatus",
            value = out(reg) value,
            options(nomem, nostack)
        );
    }

    // SPP=0：sret 目标是 U-mode。
    // sie 本课为 0，所以也清 SIE/SPIE，避免把早期实验环境的状态偷偷带入用户现场。
    value & !(SSTATUS_SPP | SSTATUS_SIE | SSTATUS_SPIE)
}

fn read_sie() -> usize {
    let value: usize;
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

fn validate_runtime_ranges(user_stack: AddressRange, trap_stack: AddressRange) {
    let (_, _, boot_stack) = stack_ranges();

    if user_stack.size() != STACK_BYTES || trap_stack.size() != STACK_BYTES {
        panic!("user/trap stack size is not 16 KiB");
    }
    if user_stack.overlaps(trap_stack)
        || user_stack.overlaps(boot_stack)
        || trap_stack.overlaps(boot_stack)
    {
        panic!("user, trap, and boot stack regions overlap");
    }
}

fn initial_user_sp(user_stack: AddressRange) -> usize {
    // end 是 one-past-end；使用 end-16 既在范围内，又满足 RISC-V ABI 16-byte 对齐。
    let user_sp = user_stack.end - 16;
    if user_sp & 0xf != 0 || !user_stack.contains(user_sp) {
        panic!("initial user sp is not a 16-byte aligned address inside user stack");
    }
    user_sp
}

fn prepare_process<'program>(
    pid: u64,
    program: &'program Program,
    user_stack: AddressRange,
    trap_stack: AddressRange,
) -> Process<'program> {
    validate_runtime_ranges(user_stack, trap_stack);

    if trap_stack.end & 0xf != 0 {
        panic!("trap stack top is not 16-byte aligned");
    }
    if program.entry() & 0x1 != 0 {
        panic!("user entry violates current RISC-V instruction alignment");
    }
    if read_sie() != 0 {
        panic!("user-mode lessons expect sie=0 before sret");
    }

    let sstatus = trusted_user_sstatus();
    if sstatus & SSTATUS_SPP != 0 {
        panic!("trusted user sstatus still has SPP=1");
    }

    let context = UserContext::new(program.entry(), initial_user_sp(user_stack), sstatus);
    let runtime = UserRuntime {
        user_stack,
        trap_stack,
        context,
    };

    let mut process = Process::new(pid, program);
    process.attach_user_runtime(runtime);
    process.start();

    if process.state() != ProcessState::Running {
        panic!("user process did not enter Running state");
    }

    process
}

fn enter_process(process: &Process<'_>) -> ! {
    let runtime = process.user_runtime();

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

// ---------- 第 07 课 ----------
pub(crate) fn run_lesson07(pid: u64) -> ! {
    let (code_start, entry, expected_ecall, code_end) = user07_code_addresses();
    let program = Program::from_linked("user-ecall", entry, code_start, code_end);
    let (user_stack, trap_stack, _) = stack_ranges();

    if !program.code_range().contains(expected_ecall) {
        panic!("lesson 07 ecall label is outside user code range");
    }

    let process = prepare_process(pid, &program, user_stack, trap_stack);
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

    enter_process(&process)
}

fn handle_lesson07(frame: &TrapFrame) -> ! {
    let (code_start, _, expected_ecall, code_end) = user07_code_addresses();
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
    let kernel_stvec_restored = read_stvec() == kernel_trap_entry_address();

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
    stop()
}

// ---------- 第 08 课 ----------
pub(crate) fn run_lesson08(pid: u64) -> ! {
    let (code_start, entry, code_end) = user08_code_addresses();
    let program = Program::from_linked("user-syscalls", entry, code_start, code_end);
    let (user_stack, trap_stack, _) = stack_ranges();

    let process = prepare_process(pid, &program, user_stack, trap_stack);
    let runtime = process.user_runtime();

    crate::println!("[syscall setup] pid={} program={}", process.id(), process.program().name());
    crate::println!("[syscall setup] abi=a7:number,a0:arg0/result");
    crate::println!("[syscall setup] spp=0");
    crate::println!("[syscall setup] sepc={:#x}", runtime.context.sepc);
    crate::println!("[syscall enter]");

    enter_process(&process)
}

fn handle_lesson08(frame: &mut TrapFrame) {
    let (code_start, _, code_end) = user08_code_addresses();
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
    let known_ecall = user08_known_ecall(frame.sepc);
    let kernel_stvec_restored = read_stvec() == kernel_trap_entry_address();

    if origin_spp != 0
        || is_interrupt
        || cause_code != 8
        || !frame_in_trap_stack
        || !user_sp_ok
        || !sepc_in_user_code
        || !known_ecall
        || !kernel_stvec_restored
    {
        panic!(
            "lesson 08 received unexpected user trap: spp={} interrupt={} cause={} sepc={:#x}",
            origin_spp,
            is_interrupt,
            cause_code,
            frame.sepc
        );
    }

    let number = frame.x[17];
    let arg0 = frame.x[10];
    let old_sepc = frame.sepc;

    // 当前已经确认这是本课受控的 32-bit `ecall`，所以成功处理后恰好跳过 4 bytes。
    let next_sepc = old_sepc
        .checked_add(4)
        .unwrap_or_else(|| panic!("syscall sepc overflow"));
    if next_sepc > code_end {
        panic!("syscall resume pc escaped user code range");
    }

    let result = crate::syscall::dispatch(number, arg0);

    // 负错误码在 RV64 a0 中只是二补码 64-bit bit pattern。
    // `as usize` 保留这组 bits；用户汇编用 `li -1/-2` 比较同一 bit pattern。
    frame.x[10] = result as usize;
    frame.sepc = next_sepc;

    // 不信任用户来源的 SPP；返回前再次强制 sret 目标为 U-mode。
    // 本课还不启用 S interrupts，因此也继续保持 SIE/SPIE 清零。
    frame.sstatus &= !(SSTATUS_SPP | SSTATUS_SIE | SSTATUS_SPIE);

    // 成功 putchar 本身就是用户可见输出，不额外插入 kernel 日志。
    // 只记录两个负例，使测试能看到 ABI 的 signed 解释和 PC 推进事实。
    if result < 0 {
        crate::println!(
            "[syscall] number={} arg0={} result={} sepc={:#x} next={:#x}",
            number,
            arg0,
            result,
            old_sepc,
            next_sepc
        );
    }
}

// 汇编在可信 trap stack 上构造完整 frame 后调用这里。
// 第 07 课不会返回；第 08 课处理 syscall 后会普通 return，让 trap.S 恢复用户现场并 sret。
#[unsafe(no_mangle)]
pub extern "C" fn rust_user_trap_handler(frame: *mut TrapFrame) {
    let frame = unsafe { &mut *frame };

    #[cfg(feature = "lesson08-syscalls")]
    {
        handle_lesson08(frame);
        return;
    }

    #[cfg(feature = "lesson07-user-mode")]
    handle_lesson07(frame);

    panic!("unexpected user trap without an active user-mode checkpoint");
}
