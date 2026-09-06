// 第 10 课：受控用户 fault 只结束当前 Process，并复用第 09 课 KernelContext 返回桥。

use core::arch::{asm, global_asm};
use core::ptr::{addr_of, write_bytes};
use core::sync::atomic::{AtomicUsize, Ordering};

use crate::process::{AddressRange, FaultInfo, Process, ProcessState, Program, UserContext, UserRuntime};
use crate::syscall::SyscallOutcome;
use crate::trap::{TrapFrame, TRAP_FRAME_SIZE};

global_asm!(include_str!("user_api.S"));
global_asm!(include_str!("user10.S"));

unsafe extern "C" {
    fn run_user(user_sp: usize, entry: usize, sstatus: usize, trap_stack_top: usize) -> usize;
    fn trap_entry();

    static user10_fault_start: u8;
    static user10_fault_entry: u8;
    static user10_fault_instruction: u8;
    static user10_fault_end: u8;

    static user10_normal_start: u8;
    static user10_normal_entry: u8;
    static user10_normal_end: u8;

    static user_api_start: u8;
    static user_api_putchar_ecall: u8;
    static user_api_exit_ecall: u8;
    static user_api_end: u8;

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
const OUTCOME_NONE: usize = 0;
const OUTCOME_EXIT: usize = 1;
const OUTCOME_FAULT: usize = 2;
const NO_EXIT_CODE: usize = usize::MAX;

static OUTCOME_KIND: AtomicUsize = AtomicUsize::new(OUTCOME_NONE);
static EXIT_CODE: AtomicUsize = AtomicUsize::new(NO_EXIT_CODE);
static FAULT_CAUSE: AtomicUsize = AtomicUsize::new(usize::MAX);
static FAULT_SEPC: AtomicUsize = AtomicUsize::new(0);
static FAULT_STVAL: AtomicUsize = AtomicUsize::new(0);

fn symbol(symbol: *const u8) -> usize {
    symbol as usize
}

fn stack_ranges() -> (AddressRange, AddressRange, AddressRange) {
    unsafe {
        (
            AddressRange::new(symbol(addr_of!(user_stack_start)), symbol(addr_of!(user_stack_end))),
            AddressRange::new(
                symbol(addr_of!(user_trap_stack_start)),
                symbol(addr_of!(user_trap_stack_end)),
            ),
            AddressRange::new(symbol(addr_of!(sboot_stack)), symbol(addr_of!(eboot_stack))),
        )
    }
}

fn trusted_user_sstatus() -> usize {
    let value: usize;
    unsafe {
        asm!("csrr {value}, sstatus", value = out(reg) value, options(nomem, nostack));
    }
    value & !(SSTATUS_SPP | SSTATUS_SIE | SSTATUS_SPIE)
}

fn read_sie() -> usize {
    let value: usize;
    unsafe {
        asm!("csrr {value}, sie", value = out(reg) value, options(nomem, nostack));
    }
    value
}

fn read_stvec() -> usize {
    let value: usize;
    unsafe {
        asm!("csrr {value}, stvec", value = out(reg) value, options(nomem, nostack));
    }
    value
}

#[inline(always)]
fn current_sp() -> usize {
    let value: usize;
    unsafe {
        asm!("mv {value}, sp", value = out(reg) value, options(nomem, nostack));
    }
    value
}

fn prepare_process<'p>(pid: u64, program: &'p Program) -> Process<'p> {
    let (user_stack, trap_stack, boot_stack) = stack_ranges();
    if user_stack.size() != STACK_BYTES || trap_stack.size() != STACK_BYTES {
        panic!("lesson 10 user/trap stack size changed");
    }
    if user_stack.overlaps(trap_stack)
        || user_stack.overlaps(boot_stack)
        || trap_stack.overlaps(boot_stack)
    {
        panic!("lesson 10 stack regions overlap");
    }
    if read_sie() != 0 {
        panic!("lesson 10 expects interrupts disabled before entering U-mode");
    }

    let user_sp = user_stack.end - 16;
    if user_sp & 0xf != 0 || !user_stack.contains(user_sp) {
        panic!("lesson 10 initial user sp is invalid");
    }

    let runtime = UserRuntime {
        user_stack,
        trap_stack,
        context: UserContext::new(program.entry(), user_sp, trusted_user_sstatus()),
    };
    let mut process = Process::new(pid, program);
    process.attach_user_runtime(runtime);
    process.start();
    process
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

fn clear_runtime_stacks() {
    let (user_stack, trap_stack, _) = stack_ranges();
    let sp = current_sp();
    if user_stack.contains(sp) || trap_stack.contains(sp) {
        panic!("lesson 10 tried to clear a live stack");
    }
    unsafe {
        write_bytes(user_stack.start as *mut u8, 0, user_stack.size());
        write_bytes(trap_stack.start as *mut u8, 0, trap_stack.size());
    }
}

fn reset_outcome() {
    OUTCOME_KIND.store(OUTCOME_NONE, Ordering::Relaxed);
    EXIT_CODE.store(NO_EXIT_CODE, Ordering::Relaxed);
    FAULT_CAUSE.store(usize::MAX, Ordering::Relaxed);
    FAULT_SEPC.store(0, Ordering::Relaxed);
    FAULT_STVAL.store(0, Ordering::Relaxed);
}

fn take_fault() -> FaultInfo {
    if OUTCOME_KIND.swap(OUTCOME_NONE, Ordering::Relaxed) != OUTCOME_FAULT {
        panic!("lesson 10 expected a fault outcome");
    }
    FaultInfo {
        cause: FAULT_CAUSE.load(Ordering::Relaxed),
        sepc: FAULT_SEPC.load(Ordering::Relaxed),
        stval: FAULT_STVAL.load(Ordering::Relaxed),
    }
}

fn take_exit() -> u8 {
    if OUTCOME_KIND.swap(OUTCOME_NONE, Ordering::Relaxed) != OUTCOME_EXIT {
        panic!("lesson 10 expected an exit outcome");
    }
    let code = EXIT_CODE.swap(NO_EXIT_CODE, Ordering::Relaxed);
    if code > u8::MAX as usize {
        panic!("lesson 10 exit outcome has invalid code");
    }
    code as u8
}

fn validate_boundary(frame: &TrapFrame) -> usize {
    let (user_stack, trap_stack, _) = stack_ranges();
    let interrupt_bit = 1usize << (usize::BITS - 1);
    let frame_addr = frame as *const TrapFrame as usize;
    let frame_end = frame_addr.checked_add(TRAP_FRAME_SIZE).unwrap_or(usize::MAX);

    if frame.scause & interrupt_bit != 0 {
        panic!("lesson 10 received an unexpected interrupt");
    }
    if frame.sstatus & SSTATUS_SPP != 0 {
        panic!("lesson 10 user trap has SPP=1");
    }
    if !user_stack.contains(frame.x[2]) || frame.x[2] & 0xf != 0 {
        panic!("lesson 10 saved user sp is invalid");
    }
    if frame_addr < trap_stack.start || frame_end > trap_stack.end {
        panic!("lesson 10 TrapFrame escaped trusted trap stack");
    }
    if read_stvec() != trap_entry as *const () as usize {
        panic!("lesson 10 did not restore kernel stvec inside trap handler");
    }

    frame.scause
}

fn in_range(address: usize, start: *const u8, end: *const u8) -> bool {
    let start = symbol(start);
    let end = symbol(end);
    start <= address && address < end
}

pub(crate) fn run_lesson10(first_pid: u64) -> ! {
    let (fault_start, fault_entry, fault_instruction, fault_end) = unsafe {
        (
            symbol(addr_of!(user10_fault_start)),
            symbol(addr_of!(user10_fault_entry)),
            symbol(addr_of!(user10_fault_instruction)),
            symbol(addr_of!(user10_fault_end)),
        )
    };
    let fault_program = Program::from_linked("fault-demo", fault_entry, fault_start, fault_end);
    let mut fault_process = prepare_process(first_pid, &fault_program);

    reset_outcome();
    let manager_sp = current_sp();
    let action = run_process(&fault_process);
    if action != ACTION_TERMINATE_PROCESS || current_sp() != manager_sp {
        panic!("lesson 10 user fault did not restore manager context");
    }
    let info = take_fault();
    if info.cause != 2 || info.sepc != fault_instruction {
        panic!("lesson 10 fault metadata does not match controlled illegal instruction");
    }
    fault_process.finish_fault(info);
    match fault_process.state() {
        ProcessState::Faulted(saved) if saved == info => {}
        _ => panic!("lesson 10 fault process did not commit Faulted"),
    }
    crate::println!(
        "[user fault] pid={} cause={} sepc={:#x} stval={:#x}",
        fault_process.id(), info.cause, info.sepc, info.stval
    );
    crate::println!("[user fault] process=Faulted kernel_alive=true");
    clear_runtime_stacks();

    let second_pid = first_pid.checked_add(1).unwrap_or_else(|| panic!("PID overflow"));
    let (normal_start, normal_entry, normal_end) = unsafe {
        (
            symbol(addr_of!(user10_normal_start)),
            symbol(addr_of!(user10_normal_entry)),
            symbol(addr_of!(user10_normal_end)),
        )
    };
    let normal_program = Program::from_linked("after-fault", normal_entry, normal_start, normal_end);
    let mut normal_process = prepare_process(second_pid, &normal_program);
    reset_outcome();
    let manager_sp = current_sp();
    let action = run_process(&normal_process);
    if action != ACTION_TERMINATE_PROCESS || current_sp() != manager_sp {
        panic!("lesson 10 normal exit did not restore manager context");
    }
    let code = take_exit();
    normal_process.finish_exit(code);
    if !matches!(normal_process.state(), ProcessState::Exited(0)) {
        panic!("lesson 10 post-fault process did not exit 0");
    }
    crate::println!("[user fault] pid={} process=Exited(0)", normal_process.id());
    crate::println!("[stage 02] complete");
    clear_runtime_stacks();

    loop {
        unsafe { asm!("wfi") };
    }
}

pub(crate) fn handle_user_trap(frame: &mut TrapFrame) -> usize {
    let cause = validate_boundary(frame);
    let pc = frame.sepc;

    let fault_pc = unsafe { symbol(addr_of!(user10_fault_instruction)) };
    if cause == 2 && pc == fault_pc {
        FAULT_CAUSE.store(cause, Ordering::Relaxed);
        FAULT_SEPC.store(frame.sepc, Ordering::Relaxed);
        FAULT_STVAL.store(frame.stval, Ordering::Relaxed);
        OUTCOME_KIND.store(OUTCOME_FAULT, Ordering::Relaxed);
        return ACTION_TERMINATE_PROCESS;
    }

    if cause != 8 {
        panic!("lesson 10 unexpected user exception cause={} sepc={:#x}", cause, pc);
    }

    let (api_start, putchar_ecall, exit_ecall, api_end) = unsafe {
        (
            addr_of!(user_api_start),
            symbol(addr_of!(user_api_putchar_ecall)),
            symbol(addr_of!(user_api_exit_ecall)),
            addr_of!(user_api_end),
        )
    };
    if !in_range(pc, api_start, api_end) || (pc != putchar_ecall && pc != exit_ecall) {
        panic!("lesson 10 ecall occurred outside user API wrapper: {:#x}", pc);
    }

    let number = frame.x[17];
    let arg0 = frame.x[10];
    match crate::syscall::dispatch(number, arg0) {
        SyscallOutcome::Return(result) => {
            frame.x[10] = result as usize;
            frame.sepc = frame.sepc.checked_add(4).unwrap_or_else(|| panic!("sepc overflow"));
            frame.sstatus &= !(SSTATUS_SPP | SSTATUS_SIE | SSTATUS_SPIE);
            ACTION_RESUME_USER
        }
        SyscallOutcome::Exit(code) => {
            EXIT_CODE.store(code as usize, Ordering::Relaxed);
            OUTCOME_KIND.store(OUTCOME_EXIT, Ordering::Relaxed);
            ACTION_TERMINATE_PROCESS
        }
    }
}
