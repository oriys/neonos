# Lab 2 提示

<details><summary>第一层：先画两张状态图</summary>

一张是 Process 的 Ready/Running/Exited/Faulted，另一张是 handler 返回 ResumeUser/TerminateProcess 后汇编会去哪里。未知 syscall 是协议错误，不是 CPU 异常。
</details>

<details><summary>第二层：返回寄存器与共享状态</summary>

frame.x[10] 是保存的 a0。frame.sepc 只有处理确认的 ecall 才 +4。RunOutcome 先放入 ACTIVE；Process::finish_exit/finish_fault 由 run_one 在回到管理栈后执行。不要跨 run_user 保存指向旧 trap 栈的 Rust 引用。
</details>

<details><summary>第三层：内核故障钩子怎么保留？</summary>

在完成可信来源检查后，用 `#[cfg(feature = "lesson10-kernel-fault")]` 条件编译一次 `unsafe { trigger_illegal_instruction() };`。该函数在 S-mode 执行，user_trap_entry 已将 stvec 切回内核入口。这个钩子验证“存在当前进程”不能替代 SPP 分类，不应在正常批次触发。
</details>
