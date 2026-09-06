// 第 06～10 课的 Program / Process 数据模型。
//
// 第 10 课第一次真正出现用户 fault，所以 `Faulted(FaultInfo)` 到现在才加入状态机。

use core::arch::global_asm;
use core::ptr::addr_of;

global_asm!(
    r#"
    .section .text.model_program, "ax"
    .balign 4
    .globl model_hello_start
model_hello_start:
    .globl model_hello_entry
model_hello_entry:
    nop
    ret
    .globl model_hello_end
model_hello_end:
"#
);

unsafe extern "C" {
    static model_hello_start: u8;
    static model_hello_entry: u8;
    static model_hello_end: u8;
}

pub(crate) struct Program {
    name: &'static str,
    entry: usize,
    code_start: usize,
    code_end: usize,
}

impl Program {
    pub(crate) fn from_linked(
        name: &'static str,
        entry: usize,
        code_start: usize,
        code_end: usize,
    ) -> Self {
        if code_start >= code_end {
            panic!(
                "program {} has invalid code range: [{:#x}, {:#x})",
                name, code_start, code_end
            );
        }
        if entry < code_start || entry >= code_end {
            panic!(
                "program {} entry is outside code range: entry={:#x}",
                name, entry
            );
        }

        Self {
            name,
            entry,
            code_start,
            code_end,
        }
    }

    fn linked_hello() -> Self {
        let (code_start, entry, code_end) = unsafe {
            (
                addr_of!(model_hello_start) as usize,
                addr_of!(model_hello_entry) as usize,
                addr_of!(model_hello_end) as usize,
            )
        };

        Self::from_linked("hello", entry, code_start, code_end)
    }

    pub(crate) fn name(&self) -> &'static str {
        self.name
    }

    pub(crate) fn entry(&self) -> usize {
        self.entry
    }

    pub(crate) fn code_range(&self) -> AddressRange {
        AddressRange {
            start: self.code_start,
            end: self.code_end,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct FaultInfo {
    pub(crate) cause: usize,
    pub(crate) sepc: usize,
    pub(crate) stval: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProcessState {
    Ready,
    Running,
    Exited(i32),
    Faulted(FaultInfo),
}

#[derive(Clone, Copy)]
struct InvalidTransition {
    from: ProcessState,
    to: ProcessState,
}

#[derive(Clone, Copy)]
pub(crate) struct AddressRange {
    pub(crate) start: usize,
    pub(crate) end: usize,
}

impl AddressRange {
    pub(crate) fn new(start: usize, end: usize) -> Self {
        if start >= end {
            panic!("invalid address range [{:#x}, {:#x})", start, end);
        }
        Self { start, end }
    }

    pub(crate) fn size(self) -> usize {
        self.end - self.start
    }

    pub(crate) fn contains(self, address: usize) -> bool {
        self.start <= address && address < self.end
    }

    pub(crate) fn overlaps(self, other: Self) -> bool {
        self.start < other.end && other.start < self.end
    }
}

#[derive(Clone, Copy)]
pub(crate) struct UserContext {
    pub(crate) x: [usize; 32],
    pub(crate) sepc: usize,
    pub(crate) sstatus: usize,
}

impl UserContext {
    pub(crate) fn new(entry: usize, user_sp: usize, sstatus: usize) -> Self {
        let mut x = [0usize; 32];
        x[2] = user_sp;
        Self {
            x,
            sepc: entry,
            sstatus,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct UserRuntime {
    pub(crate) user_stack: AddressRange,
    pub(crate) trap_stack: AddressRange,
    pub(crate) context: UserContext,
}

pub(crate) struct Process<'program> {
    id: u64,
    program: &'program Program,
    state: ProcessState,
    user: Option<UserRuntime>,
}

impl<'program> Process<'program> {
    pub(crate) fn new(id: u64, program: &'program Program) -> Self {
        Self {
            id,
            program,
            state: ProcessState::Ready,
            user: None,
        }
    }

    fn transition(&mut self, next: ProcessState) -> Result<(), InvalidTransition> {
        let current = self.state;

        let allowed = matches!(
            (current, next),
            (ProcessState::Ready, ProcessState::Running)
                | (ProcessState::Running, ProcessState::Exited(_))
                | (ProcessState::Running, ProcessState::Faulted(_))
        );

        if !allowed {
            return Err(InvalidTransition {
                from: current,
                to: next,
            });
        }

        self.state = next;
        Ok(())
    }

    pub(crate) fn start(&mut self) {
        if self.transition(ProcessState::Running).is_err() {
            panic!("process {} cannot transition to Running", self.id);
        }
    }

    pub(crate) fn finish_exit(&mut self, code: u8) {
        if self.transition(ProcessState::Exited(code as i32)).is_err() {
            panic!("process {} cannot transition Running -> Exited", self.id);
        }
    }

    pub(crate) fn finish_fault(&mut self, info: FaultInfo) {
        if self.transition(ProcessState::Faulted(info)).is_err() {
            panic!("process {} cannot transition Running -> Faulted", self.id);
        }
    }

    pub(crate) fn attach_user_runtime(&mut self, runtime: UserRuntime) {
        if self.user.is_some() {
            panic!("process {} already has user runtime resources", self.id);
        }
        self.user = Some(runtime);
    }

    pub(crate) fn user_runtime(&self) -> &UserRuntime {
        self.user
            .as_ref()
            .unwrap_or_else(|| panic!("process {} has no user runtime", self.id))
    }

    pub(crate) fn id(&self) -> u64 {
        self.id
    }

    pub(crate) fn program(&self) -> &Program {
        self.program
    }

    pub(crate) fn state(&self) -> ProcessState {
        self.state
    }
}

struct PidAllocator {
    next: u64,
}

impl PidAllocator {
    fn new(first: u64) -> Self {
        Self { next: first }
    }

    fn allocate(&mut self) -> Option<u64> {
        let id = self.next;
        self.next = self.next.checked_add(1)?;
        Some(id)
    }

    fn next_id(&self) -> u64 {
        self.next
    }
}

fn print_state(process: &Process<'_>) {
    match process.state {
        ProcessState::Ready => crate::println!("[model] pid={} Ready", process.id),
        ProcessState::Running => crate::println!("[model] pid={} Running", process.id),
        ProcessState::Exited(code) => {
            crate::println!("[model] pid={} Exited({})", process.id, code)
        }
        ProcessState::Faulted(info) => crate::println!(
            "[model] pid={} Faulted(cause={},sepc={:#x},stval={:#x})",
            process.id,
            info.cause,
            info.sepc,
            info.stval
        ),
    }
}

fn state_name(state: ProcessState) -> &'static str {
    match state {
        ProcessState::Ready => "Ready",
        ProcessState::Running => "Running",
        ProcessState::Exited(_) => "Exited",
        ProcessState::Faulted(_) => "Faulted",
    }
}

pub(crate) fn run_lesson06_model() -> u64 {
    let program = Program::linked_hello();
    crate::println!(
        "[model] program={} entry={:#x} code=[{:#x}, {:#x})",
        program.name,
        program.entry,
        program.code_start,
        program.code_end
    );

    let mut pids = PidAllocator::new(1);

    let pid1 = pids
        .allocate()
        .unwrap_or_else(|| panic!("PID allocator overflow"));
    let mut first = Process::new(pid1, &program);
    print_state(&first);

    if first.transition(ProcessState::Running).is_err() {
        panic!("Ready -> Running was rejected");
    }
    print_state(&first);

    if first.transition(ProcessState::Exited(0)).is_err() {
        panic!("Running -> Exited(0) was rejected");
    }
    print_state(&first);

    let before = first.state;
    let error = match first.transition(ProcessState::Running) {
        Ok(()) => panic!("Exited -> Running unexpectedly succeeded"),
        Err(error) => error,
    };

    if first.state != before {
        panic!("invalid process transition changed the original state");
    }

    let exit_code = match first.state {
        ProcessState::Exited(code) => code,
        _ => panic!("pid=1 did not remain Exited after invalid transition"),
    };

    crate::println!(
        "[model] pid={} reject {} -> {} kept=Exited({})",
        first.id,
        state_name(error.from),
        state_name(error.to),
        exit_code
    );

    let pid2 = pids
        .allocate()
        .unwrap_or_else(|| panic!("PID allocator overflow"));
    let mut second = Process::new(pid2, &program);
    print_state(&second);

    crate::println!(
        "[model] shared_program={}",
        core::ptr::eq(first.program, second.program)
    );

    if second.transition(ProcessState::Running).is_err() {
        panic!("second Ready -> Running was rejected");
    }
    print_state(&second);

    if second.transition(ProcessState::Exited(0)).is_err() {
        panic!("second Running -> Exited(0) was rejected");
    }
    print_state(&second);

    if first.id == second.id {
        panic!("two process instances received the same PID");
    }

    crate::println!("[model] process model ok");
    pids.next_id()
}
