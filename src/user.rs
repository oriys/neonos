// 第 07～09 课：真实 U-mode 运行、syscall 返回，以及 exit 后恢复内核管理调用链。

use core::arch::{asm, global_asm};
use core::ptr::{addr_of, write_bytes};
use core::sync::atomic::{AtomicUsize, Ordering};

use crate::process::{AddressRange, Process, ProcessState, Program, UserContext, UserRuntime};
use crate::syscall::SyscallOutcome;
use crate::trap::{
    KCTX_GP, KCTX_RA, KCTX_S0, KCTX_S1, KCTX_S2, KCTX_S3, KCTX_S4, KCTX_S5, KCTX_S6, KCTX_S7,
    KCTX_S8, KCTX_S9, KCTX_S10, KCTX_S11, KCTX_SP, KCTX_TP, KERNEL_CONTEXT_SIZE, TRAP_FRAME_SIZE,
    TrapFrame,
};

global_asm!(
    include_str!("user.S"),
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

#[cfg(any(
    all(
        feature = "lesson10-user-errors",
        any(
            feature = "lesson07-user-mode",
            feature = "lesson08-syscalls",
            feature = "lesson09-exit"
        )
    ),
    all(feature = "lesson07-user-mode", feature = "lesson08-syscalls"),
    all(feature = "lesson07-user-mode", feature = "lesson09-exit"),
    all(feature = "lesson08-syscalls", feature = "lesson09-exit")
))]
compile_error!(
    "lesson07-user-mode, lesson08-syscalls, and lesson09-exit are mutually exclusive checkpoints"
);

unsafe extern "C" {
    fn enter_user_mode(user_sp: usize, entry: usize, sstatus: usize, trap_stack_top: usize);
    fn run_user(user_sp: usize, entry: usize, sstatus: usize, trap_stack_top: usize) -> usize;
    fn trap_entry();

    static user_start: u8;
    static user_entry: u8;
    static user_ecall: u8;
    static user_end: u8;

    static user08_start: u8;
    static user08_entry: u8;
    static user08_ecall_unknown: u8;
    static user08_ecall_invalid: u8;
    static user08_ecall_o: u8;
    static user08_ecall_k: u8;
    static user08_ecall_newline: u8;
    static user08_end: u8;

    static user09_start: u8;
    static user09_entry: u8;
    static user09_ecall_invalid_exit: u8;
    static user09_ecall_exit7: u8;
    static user09_end: u8;

    static user09_stress_start: u8;
    static user09_stress_entry: u8;
    static user09_stress_ecall: u8;
    static user09_stress_end: u8;

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

const ACTION_RESUME_USER: usize = 0;
const ACTION_TERMINATE_PROCESS: usize = 1;
const NO_EXIT_CODE: usize = usize::MAX;

// 单 hart、一次只运行一个 Process 的第 09 课可以用两个原子 mailbox 保存跨 trap/run_user 的事实。
// 它们不是未来 process table 的替代品；只是当前阶段的最小可信桥接状态。
static LAST_EXIT_CODE: AtomicUsize = AtomicUsize::new(NO_EXIT_CODE);
static EXPECTED_TRAP_FRAME: AtomicUsize = AtomicUsize::new(0);

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

fn user09_code_addresses() -> (usize, usize, usize) {
    unsafe {
        (
            symbol_address(addr_of!(user09_start)),
            symbol_address(addr_of!(user09_entry)),
            symbol_address(addr_of!(user09_end)),
        )
    }
}

fn user09_stress_code_addresses() -> (usize, usize, usize) {
    unsafe {
        (
            symbol_address(addr_of!(user09_stress_start)),
            symbol_address(addr_of!(user09_stress_entry)),
            symbol_address(addr_of!(user09_stress_end)),
        )
    }
}

fn user09_ecall_kind(address: usize) -> Option<&'static str> {
    unsafe {
        if address == symbol_address(addr_of!(user09_ecall_invalid_exit)) {
            Some("invalid")
        } else if address == symbol_address(addr_of!(user09_ecall_exit7)) {
            Some("exit7")
        } else if address == symbol_address(addr_of!(user09_stress_ecall)) {
            Some("stress")
        } else {
            None
        }
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

#[inline(always)]
fn current_sp() -> usize {
    let value: usize;
    unsafe {
        asm!(
            "mv {value}, sp",
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

fn run_process(process: &Process<'_>) -> usize {
    let runtime = process.user_runtime();
    unsafe {
        run_user(
            runtime.context.x[2],
            runtime.context.sepc,
            runtime.context.sstatus,
            runtime.trap_stack.end,
        )
    }
}

fn validate_user_trap(frame: &TrapFrame, code_start: usize, code_end: usize) -> usize {
    let (user_stack, trap_stack, _) = stack_ranges();
    let interrupt_bit = 1usize << (usize::BITS - 1);
    let is_interrupt = frame.scause & interrupt_bit != 0;
    let cause_code = frame.scause & !interrupt_bit;
    let origin_spp = (frame.sstatus & SSTATUS_SPP) >> 8;
    let frame_address = frame as *const TrapFrame as usize;
    let frame_end = frame_address
        .checked_add(TRAP_FRAME_SIZE)
        .unwrap_or(usize::MAX);
    let frame_in_trap_stack = trap_stack.start <= frame_address && frame_end <= trap_stack.end;
    let user_sp_ok = user_stack.contains(frame.x[2]) && frame.x[2] & 0xf == 0;
    let sepc_in_user_code = code_start <= frame.sepc && frame.sepc < code_end;
    let kernel_stvec_restored = read_stvec() == kernel_trap_entry_address();

    if origin_spp != 0
        || is_interrupt
        || cause_code != 8
        || !frame_in_trap_stack
        || !user_sp_ok
        || !sepc_in_user_code
        || !kernel_stvec_restored
    {
        panic!(
            "unexpected user trap: spp={} interrupt={} cause={} sepc={:#x}",
            origin_spp, is_interrupt, cause_code, frame.sepc
        );
    }

    frame_address
}

fn apply_return(frame: &mut TrapFrame, result: isize, code_end: usize) {
    let old_sepc = frame.sepc;
    let next_sepc = old_sepc
        .checked_add(4)
        .unwrap_or_else(|| panic!("syscall sepc overflow"));
    if next_sepc > code_end {
        panic!("syscall resume pc escaped user code range");
    }

    frame.x[10] = result as usize;
    frame.sepc = next_sepc;
    frame.sstatus &= !(SSTATUS_SPP | SSTATUS_SIE | SSTATUS_SPIE);
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

    crate::println!(
        "[user setup] pid={} program={}",
        process.id(),
        process.program().name()
    );
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
    let frame_end = frame_address
        .checked_add(TRAP_FRAME_SIZE)
        .unwrap_or(usize::MAX);
    let frame_in_trap_stack = trap_stack.start <= frame_address && frame_end <= trap_stack.end;
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

    crate::println!(
        "[syscall setup] pid={} program={}",
        process.id(),
        process.program().name()
    );
    crate::println!("[syscall setup] abi=a7:number,a0:arg0/result");
    crate::println!("[syscall setup] spp=0");
    crate::println!("[syscall setup] sepc={:#x}", runtime.context.sepc);
    crate::println!("[syscall enter]");

    enter_process(&process)
}

fn handle_lesson08(frame: &mut TrapFrame) {
    let (code_start, _, code_end) = user08_code_addresses();
    validate_user_trap(frame, code_start, code_end);

    if !user08_known_ecall(frame.sepc) {
        panic!(
            "lesson 08 trapped at an unexpected user PC {:#x}",
            frame.sepc
        );
    }

    let number = frame.x[17];
    let arg0 = frame.x[10];
    let old_sepc = frame.sepc;

    match crate::syscall::dispatch(number, arg0) {
        SyscallOutcome::Return(result) => {
            apply_return(frame, result, code_end);
            if result < 0 {
                crate::println!(
                    "[syscall] number={} arg0={} result={} sepc={:#x} next={:#x}",
                    number,
                    arg0,
                    result,
                    old_sepc,
                    frame.sepc
                );
            }
        }
        SyscallOutcome::Exit(_) => panic!("lesson 08 unexpectedly received exit syscall"),
    }
}

// ---------- 第 09 课 ----------
fn reset_exit_mailbox() {
    LAST_EXIT_CODE.store(NO_EXIT_CODE, Ordering::Relaxed);
}

fn take_exit_code() -> u8 {
    let value = LAST_EXIT_CODE.swap(NO_EXIT_CODE, Ordering::Relaxed);
    if value > u8::MAX as usize {
        panic!("run_user returned Terminate without a valid exit code");
    }
    value as u8
}

fn record_trap_frame(address: usize) {
    let expected = EXPECTED_TRAP_FRAME.load(Ordering::Relaxed);
    if expected == 0 {
        EXPECTED_TRAP_FRAME.store(address, Ordering::Relaxed);
    } else if expected != address {
        panic!(
            "trap frame drifted: first={:#x} current={:#x}",
            expected, address
        );
    }
}

fn clear_runtime_stacks(user_stack: AddressRange, trap_stack: AddressRange) {
    let sp = current_sp();
    if user_stack.contains(sp) || trap_stack.contains(sp) {
        panic!("attempted to clear a stack while still executing on it");
    }

    unsafe {
        write_bytes(user_stack.start as *mut u8, 0, user_stack.size());
        write_bytes(trap_stack.start as *mut u8, 0, trap_stack.size());
    }
}

pub(crate) fn run_lesson09(first_pid: u64) -> ! {
    EXPECTED_TRAP_FRAME.store(0, Ordering::Relaxed);

    let (user_stack, trap_stack, _) = stack_ranges();
    let (code_start, entry, code_end) = user09_code_addresses();
    let program = Program::from_linked("exit-demo", entry, code_start, code_end);

    let mut process = prepare_process(first_pid, &program, user_stack, trap_stack);
    reset_exit_mailbox();

    let manager_sp_before = current_sp();
    let action = run_process(&process);
    let manager_sp_after = current_sp();

    if action != ACTION_TERMINATE_PROCESS {
        panic!("run_user returned unexpected action {}", action);
    }
    let exit_code = take_exit_code();
    if exit_code != 7 {
        panic!("exit-demo returned {}, expected 7", exit_code);
    }
    process.finish_exit(exit_code);

    crate::println!("kernel resumed: exit={}", exit_code);
    match process.state() {
        ProcessState::Exited(7) => crate::println!("process state=Exited(7)"),
        _ => panic!("exit-demo process did not commit Exited(7)"),
    }
    crate::println!(
        "kernel_sp_returned={}",
        manager_sp_before == manager_sp_after
    );

    // 现在已经回到 management/boot stack，才允许清理曾经使用的 user/trap stacks。
    clear_runtime_stacks(user_stack, trap_stack);

    // B 次：连续运行 100 个全新 Process 实例。它们共享同一 Program 和固定栈地址，
    // 但 PID/state/context 每次重新构造。
    let (stress_start, stress_entry, stress_end) = user09_stress_code_addresses();
    let stress_program =
        Program::from_linked("exit-stress", stress_entry, stress_start, stress_end);

    let stress_first_pid = first_pid
        .checked_add(1)
        .unwrap_or_else(|| panic!("PID overflow"));
    let manager_sp_baseline = current_sp();
    let mut kernel_sp_stable = manager_sp_before == manager_sp_after;
    let mut no_running = true;
    let mut last_pid = stress_first_pid;

    for index in 0..100u64 {
        let pid = stress_first_pid
            .checked_add(index)
            .unwrap_or_else(|| panic!("PID overflow during stress test"));
        last_pid = pid;

        let mut child = prepare_process(pid, &stress_program, user_stack, trap_stack);
        reset_exit_mailbox();

        let before = current_sp();
        let action = run_process(&child);
        let after = current_sp();

        if action != ACTION_TERMINATE_PROCESS {
            panic!("stress run {} did not return Terminate", index);
        }
        let code = take_exit_code();
        if code != 0 {
            panic!("stress run {} exited with {}", index, code);
        }
        child.finish_exit(code);

        kernel_sp_stable &= before == after && after == manager_sp_baseline;
        no_running &= matches!(child.state(), ProcessState::Exited(0));

        // 只有回到 manager stack 后才清零两类用户运行栈，下一实例从干净状态开始。
        clear_runtime_stacks(user_stack, trap_stack);
    }

    crate::println!("[exit stress] completed=100");
    crate::println!(
        "[exit stress] first_pid={} last_pid={}",
        stress_first_pid,
        last_pid
    );
    crate::println!("[exit stress] kernel_sp_stable={}", kernel_sp_stable);
    crate::println!("[exit stress] trap_frame_stable=true");
    crate::println!("[exit stress] no_running={}", no_running);

    if !kernel_sp_stable || !no_running {
        panic!("lesson 09 stress invariants failed");
    }

    stop()
}

fn handle_lesson09(frame: &mut TrapFrame) -> usize {
    let (main_start, _, main_end) = user09_code_addresses();
    let (stress_start, _, stress_end) = user09_stress_code_addresses();

    let in_main = main_start <= frame.sepc && frame.sepc < main_end;
    let in_stress = stress_start <= frame.sepc && frame.sepc < stress_end;
    let (code_start, code_end) = if in_main {
        (main_start, main_end)
    } else if in_stress {
        (stress_start, stress_end)
    } else {
        panic!(
            "lesson 09 sepc outside supported user programs: {:#x}",
            frame.sepc
        );
    };

    let frame_address = validate_user_trap(frame, code_start, code_end);
    record_trap_frame(frame_address);

    let kind = user09_ecall_kind(frame.sepc)
        .unwrap_or_else(|| panic!("lesson 09 trapped at unknown ecall PC {:#x}", frame.sepc));

    let number = frame.x[17];
    let arg0 = frame.x[10];
    let old_sepc = frame.sepc;

    match crate::syscall::dispatch(number, arg0) {
        SyscallOutcome::Return(result) => {
            apply_return(frame, result, code_end);
            if kind == "invalid" {
                crate::println!(
                    "[exit] invalid code={} result={} sepc={:#x} next={:#x}",
                    arg0,
                    result,
                    old_sepc,
                    frame.sepc
                );
            }
            ACTION_RESUME_USER
        }
        SyscallOutcome::Exit(code) => {
            LAST_EXIT_CODE.store(code as usize, Ordering::Relaxed);

            // 有效 exit 不需要为了“返回用户”前移 sepc；我们直接放弃这个 TrapFrame。
            if kind == "exit7" {
                crate::println!("[exit] code={} terminate sepc={:#x}", code, old_sepc);
            }
            ACTION_TERMINATE_PROCESS
        }
    }
}

// Rust handler 只决定 Resume/Terminate；真正恢复哪套现场由 trap.S 完成。
#[unsafe(no_mangle)]
pub extern "C" fn rust_user_trap_handler(frame: *mut TrapFrame) -> usize {
    let frame = unsafe { &mut *frame };

    #[cfg(feature = "scheduling")]
    return crate::scheduler::handle(frame);

    #[cfg(feature = "lesson10-user-errors")]
    return lesson10::handle(frame);

    #[cfg(feature = "lesson09-exit")]
    return handle_lesson09(frame);

    #[cfg(feature = "lesson08-syscalls")]
    {
        handle_lesson08(frame);
        return ACTION_RESUME_USER;
    }

    #[cfg(feature = "lesson07-user-mode")]
    handle_lesson07(frame);

    panic!("unexpected user trap without an active user-mode checkpoint");
}

#[cfg(feature = "lesson10-user-errors")]
#[path = "user10.rs"]
mod lesson10;
#[cfg(feature = "lesson10-user-errors")]
pub(crate) use lesson10::run as run_lesson10;

#[cfg(all(
    feature = "scheduling",
    any(
        feature = "lesson07-user-mode",
        feature = "lesson08-syscalls",
        feature = "lesson09-exit",
        feature = "lesson10-user-errors"
    )
))]
compile_error!("scheduling and single-process user checkpoints are mutually exclusive");
