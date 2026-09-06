// 第 06 课：先建立“程序”和“进程”的最小数据模型。
//
// 这一课故意不进入 U-mode、不分配用户栈、不做调度。
// 目标只有一个：让代码结构本身体现
//
// Program = 一份稳定的代码描述
// Process = 使用某个 Program 的一次运行实例
//
// 同一个 Program 可以被两个不同 PID 的 Process 复用；
// Process 的运行状态和退出码不会反过来修改 Program。

use core::arch::global_asm;
use core::ptr::addr_of;

// 放一小段“已经真实链接进 ELF 的受控程序描述”。
// 第 06 课不会执行它；这里只用这些符号证明 Program.entry/code range
// 来自真实链接结果，而不是凭空手写一个看起来像地址的整数。
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

// Program 只描述“这是什么代码、代码在哪里”。
// 它没有 PID、当前寄存器、状态或退出码。
pub struct Program {
    name: &'static str,
    entry: usize,
    code_start: usize,
    code_end: usize,
}

impl Program {
    fn linked_hello() -> Self {
        // SAFETY: 三个符号都由上面的汇编片段导出；这里只取地址，不解引用。
        let (code_start, entry, code_end) = unsafe {
            (
                addr_of!(model_hello_start) as usize,
                addr_of!(model_hello_entry) as usize,
                addr_of!(model_hello_end) as usize,
            )
        };

        if code_start >= code_end {
            panic!(
                "model program has invalid code range: [{:#x}, {:#x})",
                code_start,
                code_end
            );
        }
        if entry < code_start || entry >= code_end {
            panic!(
                "model program entry is outside code range: entry={:#x}",
                entry
            );
        }

        Self {
            name: "hello",
            entry,
            code_start,
            code_end,
        }
    }
}

// 第 06 课只保留当前真正会出现的三种状态。
// `Faulted` 到第 10 课真实出现用户 fault 时才加入；
// `Blocked` 到真正出现等待机制时才加入。
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ProcessState {
    Ready,
    Running,
    Exited(i32),
}

// 非法转换会把“原状态”和“请求的新状态”一起返回，方便测试和诊断。
#[derive(Clone, Copy)]
pub struct InvalidTransition {
    from: ProcessState,
    to: ProcessState,
}

// Process 是一次运行实例，所以 PID 和 state 属于它。
// Program 通过共享引用复用；Process 退出不会修改 Program。
pub struct Process<'program> {
    id: u64,
    program: &'program Program,
    state: ProcessState,
}

impl<'program> Process<'program> {
    fn new(id: u64, program: &'program Program) -> Self {
        Self {
            id,
            program,
            state: ProcessState::Ready,
        }
    }

    // 所有状态变更都经过同一个入口，不在调用点随便 `state = ...`。
    fn transition(&mut self, next: ProcessState) -> Result<(), InvalidTransition> {
        let current = self.state;

        let allowed = matches!(
            (current, next),
            (ProcessState::Ready, ProcessState::Running)
                | (ProcessState::Running, ProcessState::Exited(_))
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
}

// PID 也不直接等于某个固定数组下标。
// 本课只需要一个局部、单调增加的运行编号分配器。
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
}

fn print_state(process: &Process<'_>) {
    match process.state {
        ProcessState::Ready => crate::println!("[model] pid={} Ready", process.id),
        ProcessState::Running => crate::println!("[model] pid={} Running", process.id),
        ProcessState::Exited(code) => {
            crate::println!("[model] pid={} Exited({})", process.id, code)
        }
    }
}

fn state_name(state: ProcessState) -> &'static str {
    match state {
        ProcessState::Ready => "Ready",
        ProcessState::Running => "Running",
        ProcessState::Exited(_) => "Exited",
    }
}

// 这是课程明确标记为 `[model]` 的模拟，不伪装成真实 U-mode 执行。
pub fn run_lesson06_model() {
    let program = Program::linked_hello();
    crate::println!(
        "[model] program={} entry={:#x} code=[{:#x}, {:#x})",
        program.name,
        program.entry,
        program.code_start,
        program.code_end
    );

    let mut pids = PidAllocator::new(1);

    let pid1 = pids.allocate().unwrap_or_else(|| panic!("PID allocator overflow"));
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

    // 故意请求非法的 Exited -> Running。
    // transition 必须返回错误，而且不能先把 state 改坏再报错。
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

    // 第二个 Process 与第一个 Process 共享同一个 Program 引用，但 PID/state 独立。
    let pid2 = pids.allocate().unwrap_or_else(|| panic!("PID allocator overflow"));
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
}
