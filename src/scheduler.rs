//! Lessons 12–15: one central management loop; all user traps return by value.
//! Kernel execution is single-hart, SIE=0; there are no references across stacks.
use crate::{timer, trap::TrapFrame};
use core::{
    arch::{asm, global_asm},
    cell::UnsafeCell,
    ptr::addr_of,
};

global_asm!(include_str!("user_scheduler.S"));
unsafe extern "C" {
    fn run_task(frame: *mut TrapFrame) -> usize;
    static sched_code_start: u8;
    static sched_code_end: u8;
    static sched_entry: u8;
    static sched_fault: u8;
}
const N: usize = 4;
const STACK: usize = 16384;
const ZERO_FRAME: TrapFrame = TrapFrame {
    x: [0; 32],
    sstatus: 0,
    sepc: 0,
    scause: 0,
    stval: 0,
};
#[repr(C, align(16))]
struct Stacks {
    user: [[u8; STACK]; N],
    trap: [[u8; STACK]; N],
}
struct SingleHart<T>(UnsafeCell<T>);
// Only one hart, supervisor interrupts disabled, raw copy access is scoped to a
// single management/trap phase. User memory never has a live Rust reference.
unsafe impl<T> Sync for SingleHart<T> {}
static STACKS: SingleHart<Stacks> = SingleHart(UnsafeCell::new(Stacks {
    user: [[0; STACK]; N],
    trap: [[0; STACK]; N],
}));
static TIMER_DONE: SingleHart<usize> = SingleHart(UnsafeCell::new(0));
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reason {
    Syscall,
    Yield,
    Timer,
    Exit(usize),
    Fault(usize),
}
#[derive(Clone, Copy)]
struct Exchange {
    expected: usize,
    frame: TrapFrame,
    reason: Reason,
    time: usize,
}
static EXCHANGE: SingleHart<Exchange> = SingleHart(UnsafeCell::new(Exchange {
    expected: 0,
    frame: ZERO_FRAME,
    reason: Reason::Syscall,
    time: 0,
}));
#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Ready,
    Running,
    Done,
}
#[derive(Clone, Copy)]
struct Task {
    frame: TrapFrame,
    state: State,
    level: usize,
    slice: usize,
    allot: usize,
    timers: usize,
    yields: usize,
    syscalls: usize,
    demotions: usize,
    preemptions: usize,
    first: Option<usize>,
    completion: usize,
    ready_since: usize,
    max_wait: usize,
}
impl Task {
    fn budget(&mut self) {
        self.slice = [timer::T, 2 * timer::T, 4 * timer::T][self.level];
        self.allot = [2 * timer::T, 4 * timer::T, usize::MAX][self.level];
    }
}
struct Ready {
    ids: [usize; N],
    len: usize,
}
impl Ready {
    fn new() -> Self {
        Self {
            ids: [0; N],
            len: 0,
        }
    }
    fn push(&mut self, id: usize) -> Result<(), &'static str> {
        if id >= N {
            return Err("bad task id");
        }
        if self.len == N {
            return Err("queue full");
        }
        if self.ids[..self.len].contains(&id) {
            return Err("duplicate task");
        }
        self.ids[self.len] = id;
        self.len += 1;
        Ok(())
    }
    fn pop(&mut self, tasks: &[Task; N], mlfq: bool) -> Option<usize> {
        if self.len == 0 {
            return None;
        }
        let pos = if mlfq {
            (0..self.len)
                .min_by_key(|&p| tasks[self.ids[p]].level)
                .unwrap()
        } else {
            0
        };
        let id = self.ids[pos];
        self.ids.copy_within(pos + 1..self.len, pos);
        self.len -= 1;
        Some(id)
    }
    fn check(&self, tasks: &[Task; N], count: usize) {
        assert!(self.len <= count);
        for &id in &self.ids[..self.len] {
            assert!(tasks[id].state == State::Ready);
        }
        for (id, t) in tasks.iter().enumerate().take(count) {
            assert_eq!(
                self.ids[..self.len].iter().filter(|&&v| v == id).count(),
                usize::from(t.state == State::Ready)
            );
        }
        assert!(
            tasks[..count]
                .iter()
                .filter(|t| t.state == State::Running)
                .count()
                <= 1
        );
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Policy {
    Yield,
    Timer(usize),
    Rr,
    Mlfq,
}
#[derive(Clone, Copy)]
struct Spec {
    mode: usize,
    iterations: usize,
    fault: bool,
}
impl Spec {
    const fn new(mode: usize, iterations: usize) -> Self {
        Self {
            mode,
            iterations,
            fault: false,
        }
    }
}

fn stack_top(id: usize, user: bool) -> usize {
    // addr_of! does not create an intermediate reference to user/DMA-like memory.
    unsafe {
        if user {
            addr_of!((*STACKS.0.get()).user[id]) as usize + STACK
        } else {
            addr_of!((*STACKS.0.get()).trap[id]) as usize + STACK
        }
    }
}
fn sp() -> usize {
    let v;
    unsafe {
        asm!("mv {}, sp",out(reg)v,options(nomem,nostack));
    }
    v
}
fn status() -> usize {
    let v: usize;
    unsafe {
        asm!("csrr {}, sstatus",out(reg)v,options(nomem,nostack));
    }
    v & !((1 << 8) | (1 << 5) | (1 << 1))
}

pub(crate) fn handle(frame: &mut TrapFrame) -> usize {
    let now = timer::now();
    let expected = unsafe { (*EXCHANGE.0.get()).expected };
    assert_eq!(
        frame as *const _ as usize, expected,
        "wrong task trap stack"
    );
    assert_eq!(frame.sstatus & (1 << 8), 0, "kernel fault on user path");
    let start = unsafe { addr_of!(sched_code_start) as usize };
    let end = unsafe { addr_of!(sched_code_end) as usize };
    assert!((start..end).contains(&frame.sepc), "task PC escaped code");
    let reason = if frame.scause >> 63 != 0 {
        assert_eq!(frame.scause & !(1usize << 63), 5, "unexpected interrupt");
        Reason::Timer
    } else if frame.scause == 8 {
        if frame.x[17] == 3 {
            frame.x[10] = 0;
            frame.sepc = frame.sepc.checked_add(4).expect("ecall PC overflow");
            Reason::Yield
        } else {
            match crate::syscall::dispatch(frame.x[17], frame.x[10]) {
                crate::syscall::SyscallOutcome::Return(value) => {
                    frame.x[10] = value as usize;
                    frame.sepc = frame.sepc.checked_add(4).expect("ecall PC overflow");
                    Reason::Syscall
                }
                crate::syscall::SyscallOutcome::Exit(code) => Reason::Exit(code as usize),
            }
        }
    } else {
        assert_eq!(frame.scause, 2, "unexpected user exception");
        Reason::Fault(frame.scause)
    };
    frame.sstatus &= !((1 << 8) | (1 << 5) | (1 << 1));
    unsafe {
        *EXCHANGE.0.get() = Exchange {
            expected,
            frame: *frame,
            reason,
            time: now,
        };
    }
    // Exact same ABI return bridge as exit/fault in lessons 09/10.
    1
}

fn batch(name: &str, policy: Policy, specs: &[Spec]) {
    assert!(!specs.is_empty() && specs.len() <= N);
    let mlfq = policy == Policy::Mlfq;
    let timed = policy != Policy::Yield;
    if timed {
        crate::sbi::init_time();
        timer::stop();
    }
    unsafe {
        core::ptr::write_bytes(STACKS.0.get(), 0, 1);
        *TIMER_DONE.0.get() = 0;
    }
    let mut tasks = [Task {
        frame: ZERO_FRAME,
        state: State::Done,
        level: 0,
        slice: timer::T,
        allot: 2 * timer::T,
        timers: 0,
        yields: 0,
        syscalls: 0,
        demotions: 0,
        preemptions: 0,
        first: None,
        completion: 0,
        ready_since: 0,
        max_wait: 0,
    }; N];
    // Exercise capacity/duplicate/empty behavior using the same queue implementation.
    let mut probe = Ready::new();
    assert!(probe.pop(&tasks, false).is_none());
    probe.push(0).unwrap();
    assert_eq!(probe.push(0), Err("duplicate task"));
    for i in 1..N {
        probe.push(i).unwrap();
    }
    assert_eq!(probe.push(0), Err("queue full"));
    for i in 0..N {
        assert_eq!(probe.pop(&tasks, false), Some(i));
    }
    assert!(probe.pop(&tasks, false).is_none());
    let mut ready = Ready::new();
    let base = timer::now();
    let mut boost = base.checked_add(20 * timer::T).unwrap();
    for (i, spec) in specs.iter().enumerate() {
        let t = &mut tasks[i];
        t.frame.sstatus = status();
        t.frame.sepc = unsafe {
            if spec.fault {
                addr_of!(sched_fault) as usize
            } else {
                addr_of!(sched_entry) as usize
            }
        };
        t.frame.x[2] = stack_top(i, true) - 16;
        t.frame.x[8] = b'A' as usize + i;
        t.frame.x[9] = spec.iterations;
        t.frame.x[20] = spec.mode;
        t.frame.x[21] = TIMER_DONE.0.get() as usize;
        t.state = State::Ready;
        t.ready_since = base;
        ready.push(i).unwrap();
    }
    crate::println!(
        "[schedule] begin={} policy={:?} tasks={}",
        name,
        policy,
        specs.len()
    );
    let mut current: Option<usize> = None;
    let mut deadline = 0;
    let mut switches = 0;
    let mut trace = [(0usize, Reason::Syscall); 64];
    let mut trace_len = 0;
    let mut dropped = 0;
    let mut boosts = 0;
    let baseline = sp();
    loop {
        ready.check(&tasks, specs.len());
        if mlfq && timer::now() >= boost {
            for t in &mut tasks[..specs.len()] {
                if t.state != State::Done {
                    t.level = 0;
                    t.budget();
                }
            }
            boosts += 1;
            let now = timer::now();
            boost = base
                .checked_add(((now - base) / (20 * timer::T) + 1) * (20 * timer::T))
                .unwrap();
        }

        if current.is_none() {
            current = ready.pop(&tasks, mlfq);
            let Some(i) = current else { break };
            tasks[i].state = State::Running;
            let now = timer::now();
            tasks[i].max_wait = tasks[i]
                .max_wait
                .max(now.saturating_sub(tasks[i].ready_since));
            tasks[i].first.get_or_insert(now);
            deadline = timer::after(timer::T);
            switches += 1;
        }
        let i = current.unwrap();
        let frame_ptr = (stack_top(i, false) - core::mem::size_of::<TrapFrame>()) as *mut TrapFrame;
        unsafe {
            frame_ptr.write(tasks[i].frame);
            (*EXCHANGE.0.get()).expected = frame_ptr as usize;
        }
        if timed {
            if mlfq {
                let available = tasks[i]
                    .slice
                    .min(tasks[i].allot)
                    .min(boost.saturating_sub(timer::now()))
                    .max(1);
                deadline = timer::after(available);
            }
            // RR ordinary syscall returns reuse the original absolute deadline.
            // Lesson 13 stops rearming once its explicit one-shot count is reached.
            if !matches!(policy,Policy::Timer(limit) if tasks[i].timers>=limit) {
                timer::arm(deadline);
            }
        }
        let entered = timer::now();
        assert_eq!(unsafe { run_task(frame_ptr) }, 1);
        assert_eq!(sp(), baseline, "scheduler management stack drift");
        let event = unsafe { *EXCHANGE.0.get() };
        tasks[i].frame = event.frame;
        if mlfq {
            let delta = event
                .time
                .checked_sub(entered)
                .expect("time went backwards");
            tasks[i].slice = tasks[i].slice.saturating_sub(delta);
            if tasks[i].level < 2 {
                tasks[i].allot = tasks[i].allot.saturating_sub(delta);
            }
        }
        let mut requeue = false;
        match event.reason {
            Reason::Exit(code) => {
                assert_eq!(code, 0, "user register/stack/checksum failure");
                tasks[i].state = State::Done;
                tasks[i].completion = event.time;
                current = None;
            }
            Reason::Fault(cause) => {
                assert!(specs[i].fault && cause == 2, "unexpected task fault");
                tasks[i].state = State::Done;
                tasks[i].completion = event.time;
                current = None;
            }
            Reason::Yield => {
                tasks[i].yields += 1;
                requeue = true;
            }
            Reason::Timer => {
                tasks[i].timers += 1;
                if let Policy::Timer(limit) = policy {
                    if tasks[i].timers == limit {
                        timer::stop();
                        unsafe {
                            *TIMER_DONE.0.get() = 1;
                        }
                    } else {
                        deadline = timer::after(timer::T);
                    }
                } else {
                    requeue = true;
                }
            }
            Reason::Syscall => {
                tasks[i].syscalls += 1;
            }
        }
        if tasks[i].state != State::Done && mlfq {
            if tasks[i].level < 2 && tasks[i].allot == 0 {
                tasks[i].level += 1;
                tasks[i].demotions += 1;
                tasks[i].budget();
                requeue = true;
            } else if tasks[i].slice == 0 {
                tasks[i].slice = [timer::T, 2 * timer::T, 4 * timer::T][tasks[i].level];
                requeue = true;
            }
            if event.time >= boost {
                requeue = true;
            }
        } else if tasks[i].state != State::Done && policy == Policy::Rr && event.time >= deadline {
            requeue = true;
        }
        if requeue {
            if matches!(event.reason, Reason::Timer | Reason::Syscall) {
                tasks[i].preemptions += 1;
            }
            tasks[i].state = State::Ready;
            tasks[i].ready_since = timer::now();
            ready.push(i).unwrap();
            current = None;
        }
        if event.reason != Reason::Syscall || requeue {
            if trace_len < trace.len() {
                trace[trace_len] = (i, event.reason);
                trace_len += 1;
            } else {
                dropped += 1;
            }
        }
    }
    if timed {
        timer::stop();
    }
    crate::println!();
    for (i, reason) in &trace[..trace_len] {
        crate::println!("[switch] task={} reason={:?}", i, reason);
    }
    for (i, t) in tasks[..specs.len()].iter().enumerate() {
        assert!(t.state == State::Done);
        if specs[i].mode == 4 {
            if let Policy::Timer(limit) = policy {
                assert_eq!(t.timers, limit);
            }
        }
        if specs[i].mode == 1 && specs[i].iterations >= 1_000_000 && timed {
            assert!(t.timers > 0, "long task never preempted");
        }
        if specs[i].mode == 2 && mlfq {
            assert!(t.demotions > 0, "yield escaped MLFQ accounting");
        }
        crate::println!(
            "[task] id={} timers={} yields={} syscalls={} demotions={} preemptions={} response={} turnaround={} max_ready_wait={}",
            i,
            t.timers,
            t.yields,
            t.syscalls,
            t.demotions,
            t.preemptions,
            t.first.unwrap() - base,
            t.completion - base,
            t.max_wait
        );
    }
    let sie: usize;
    unsafe {
        asm!("csrr {}, sie",out(reg)sie,options(nomem,nostack));
    }
    assert_eq!(sie & 32, 0);
    crate::println!(
        "[schedule] complete={} switches={} boosts={} dropped={} stack_stable=true timer_off=true",
        name,
        switches,
        boosts,
        dropped
    );
}

pub(crate) fn run() -> ! {
    // Reject incompatible feature combinations; exactly one checkpoint is needed.
    assert_eq!(
        usize::from(cfg!(feature = "lesson12-yield"))
            + usize::from(cfg!(feature = "lesson13-timer"))
            + usize::from(cfg!(feature = "lesson14-preemption"))
            + usize::from(cfg!(feature = "lesson15-mlfq")),
        1
    );
    if cfg!(feature = "lesson12-yield") {
        batch("alternating", Policy::Yield, &[Spec::new(0, 3); 2]);
        batch("single", Policy::Yield, &[Spec::new(2, 3)]);
        batch("stress", Policy::Yield, &[Spec::new(2, 500); 2]);
        batch(
            "fault",
            Policy::Yield,
            &[
                Spec {
                    mode: 1,
                    iterations: 0,
                    fault: true,
                },
                Spec::new(2, 3),
            ],
        );
    } else if cfg!(feature = "lesson13-timer") {
        batch("one-shot", Policy::Timer(1), &[Spec::new(4, 0)]);
        batch("ten-timers", Policy::Timer(10), &[Spec::new(4, 0)]);
    } else {
        let policy = if cfg!(feature = "lesson15-mlfq") {
            Policy::Mlfq
        } else {
            Policy::Rr
        };
        batch("compute", policy, &[Spec::new(1, 2_000_000); 2]);
        batch("single", policy, &[Spec::new(1, 1_000_000)]);
        batch(
            "fault-exit",
            policy,
            &[
                Spec {
                    mode: 1,
                    iterations: 0,
                    fault: true,
                },
                Spec::new(1, 1),
                Spec::new(1, 1_000_000),
            ],
        );
        batch(
            "syscall",
            policy,
            &[Spec::new(3, 5000), Spec::new(1, 1_000_000)],
        );
        if policy == Policy::Mlfq {
            let workload = [Spec::new(2, 5000), Spec::new(1, 2_000_000)];
            batch("compare-rr", Policy::Rr, &workload);
            batch("compare-mlfq", Policy::Mlfq, &workload);
        }
    }
    crate::println!("[schedule] stage-complete");
    loop {
        unsafe {
            asm!("wfi");
        }
    }
}
