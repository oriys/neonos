//! Single-hart, non-preemptible lesson 10. No mailbox borrow survives run_user.
use super::*;
use crate::process::FaultInfo;
use core::cell::UnsafeCell;

global_asm!(include_str!("user10.S"));
global_asm!(include_str!("user_api.S"));
unsafe extern "C" {
    static user10_fault_start: u8;
    static user10_fault_end: u8;
    static user10_normal_start: u8;
    static user10_normal_end: u8;
    static user_api_start: u8;
    static user_api_end: u8;
    fn trigger_illegal_instruction();
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RunOutcome {
    Exited(u8),
    Faulted {
        cause: usize,
        sepc: usize,
        stval: usize,
    },
}
#[derive(Clone, Copy)]
struct Active {
    code: AddressRange,
    outcome: Option<RunOutcome>,
}
struct Mailbox(UnsafeCell<Active>);
// One hart; SIE stays clear in S-mode. Read/write by value, never across dispatch.
unsafe impl Sync for Mailbox {}
static ACTIVE: Mailbox = Mailbox(UnsafeCell::new(Active {
    code: AddressRange { start: 0, end: 0 },
    outcome: None,
}));

pub(super) fn handle(frame: &mut TrapFrame) -> usize {
    let (_, trap_stack, _) = stack_ranges();
    let address = frame as *const _ as usize;
    assert!(address == trap_stack.end - TRAP_FRAME_SIZE);
    record_trap_frame(address);
    // Check hardware origin before interpreting the current process or its PC.
    assert_eq!(
        frame.sstatus & SSTATUS_SPP,
        0,
        "kernel source on user vector"
    );
    assert_eq!(frame.scause >> 63, 0, "unexpected interrupt in lesson 10");
    assert_eq!(read_stvec(), kernel_trap_entry_address());
    let active = unsafe { *ACTIVE.0.get() };
    let api = unsafe {
        AddressRange::new(
            addr_of!(user_api_start) as usize,
            addr_of!(user_api_end) as usize,
        )
    };
    assert!(active.code.contains(frame.sepc) || api.contains(frame.sepc));

    // Fault while a real process is active must still use the kernel vector.
    #[cfg(feature = "lesson10-kernel-fault")]
    unsafe {
        trigger_illegal_instruction()
    };

    let outcome = if frame.scause == 8 {
        match crate::syscall::dispatch(frame.x[17], frame.x[10]) {
            SyscallOutcome::Return(result) => {
                if result < 0 {
                    crate::println!(
                        "[user error] syscall={} arg={} result={}",
                        frame.x[17],
                        frame.x[10],
                        result
                    );
                }
                let end = if api.contains(frame.sepc) {
                    api.end
                } else {
                    active.code.end
                };
                apply_return(frame, result, end);
                return ACTION_RESUME_USER;
            }
            SyscallOutcome::Exit(code) => RunOutcome::Exited(code),
        }
    } else if frame.scause == 2 {
        RunOutcome::Faulted {
            cause: frame.scause,
            sepc: frame.sepc,
            stval: frame.stval,
        }
    } else {
        panic!(
            "unsupported user exception: cause={} sepc={:#x}",
            frame.scause, frame.sepc
        );
    };
    unsafe { (*ACTIVE.0.get()).outcome = Some(outcome) };
    ACTION_TERMINATE_PROCESS
}

fn run_one(pid: u64, name: &'static str, start: usize, end: usize) -> RunOutcome {
    let program = Program::from_linked(name, start, start, end);
    let (user_stack, trap_stack, _) = stack_ranges();
    clear_runtime_stacks(user_stack, trap_stack);
    let mut process = prepare_process(pid, &program, user_stack, trap_stack);
    unsafe {
        *ACTIVE.0.get() = Active {
            code: program.code_range(),
            outcome: None,
        }
    };
    let before = current_sp();
    assert_eq!(run_process(&process), ACTION_TERMINATE_PROCESS);
    assert_eq!(before, current_sp(), "manager stack drift");
    let outcome = unsafe { (*ACTIVE.0.get()).outcome.take() }.expect("missing user completion");
    match outcome {
        RunOutcome::Exited(code) => process.finish_exit(code),
        RunOutcome::Faulted { cause, sepc, stval } => {
            process.finish_fault(FaultInfo { cause, sepc, stval });
            crate::println!(
                "[user fault] pid={} program={} cause={} sepc={:#x} stval={:#x}",
                pid,
                name,
                cause,
                sepc,
                stval
            );
        }
    }
    assert!(!matches!(
        process.state(),
        ProcessState::Ready | ProcessState::Running
    ));
    clear_runtime_stacks(user_stack, trap_stack);
    outcome
}

pub(crate) fn run(first_pid: u64) -> ! {
    EXPECTED_TRAP_FRAME.store(0, Ordering::Relaxed);
    // Reuse lesson 09's actual invalid-exit/exit(7) program in the new dispatcher.
    let (start, _, end) = user09_code_addresses();
    assert_eq!(
        run_one(first_pid, "exit7", start, end),
        RunOutcome::Exited(7)
    );
    crate::println!("[user batch] Exited(7)");
    unsafe {
        let start = addr_of!(user10_fault_start) as usize;
        let end = addr_of!(user10_fault_end) as usize;
        assert!(
            matches!(run_one(first_pid + 1, "illegal", start, end), RunOutcome::Faulted { cause: 2, sepc, .. } if sepc == start)
        );
        assert_eq!(
            run_one(
                first_pid + 2,
                "after-fault",
                addr_of!(user10_normal_start) as usize,
                addr_of!(user10_normal_end) as usize
            ),
            RunOutcome::Exited(0)
        );
    }
    let (start, _, end) = user09_stress_code_addresses();
    for i in 0..100 {
        assert_eq!(
            run_one(first_pid + 3 + i, "stress", start, end),
            RunOutcome::Exited(0)
        );
    }
    crate::println!("[user batch] completed=100 stack_stable=true");
    crate::println!("[user batch] stage-complete");
    stop()
}
