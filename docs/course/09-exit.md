# 第 09 课：程序退出后，内核继续做什么

状态：待开始。前置：[第 08 课](08-syscalls.md) 验收完成。分 A、B 两次，每次 45～60 分钟。

## 这课解决什么问题

第 08 课所有合法 syscall 最终都：

```text
处理完成
  ↓
sret
回到同一个用户程序
```

但 `exit(7)` 的语义恰好相反：

```text
用户请求 exit
  ↓
内核记录结果
  ↓
不再恢复这个用户现场
  ↓
回到可信的内核管理流程
```

本课还没有父子进程、`wait`、fork 和调度队列。只用一个固定进程槽理解“开始一次运行 → 结束 → 回收 → 再开始下一次”。

## 先写清 exit 协议

沿用阶段协议：

```text
a7 = 2
 a0 = exit code
```

规则：

- `0 <= a0 <= 255`：有效，Process 进入 `Exited(code)`，这个 syscall **不返回用户**。
- 其他值：返回 `-2`，和普通错误 syscall 一样恢复用户继续执行。

一个很重要的差别：

```text
有效 exit
→ 不需要为了返回用户而 sepc += 4

非法 exit 参数
→ 仍然是一次需要返回的已处理 ecall
→ 按第 08 课规则前移 sepc
```

不要把两条路径写成同一个“先改 sepc，再看要不要退出”的模糊流程。

---

## A 次：建立一个安全的内核继续点

### 为什么不能随便跳回某个 Rust 函数

现在控制流大致是：

```text
rust_main / 内核管理逻辑
  ↓
进入用户
  ↓
用户 ecall
  ↓
trap 汇编
  ↓
Rust trap handler
```

有效 `exit` 时，如果从 trap handler 任意 `jump` 到一个 Rust 函数地址，相当于绕过正常函数调用栈和 ABI，Rust 栈帧、返回地址、需要恢复的寄存器都可能不匹配。

本课采用更清楚的桥接：

```text
Rust 管理函数
  ↓ 正常 call
run_user 汇编桥
  ↓ 保存 KernelContext
进入用户
  ↓
用户 exit trap
  ↓ Rust handler 正常返回“Terminate”动作给汇编
汇编恢复 KernelContext
  ↓ 回到 run_user 的内部 resume 标签
run_user 正常 ret
  ↓
原 Rust 管理函数继续
```

关键点：**不是从任意 Rust 栈帧做非局部跳转。** 被暂停的是一个设计好的 `run_user` 汇编桥，它有成对的进入和恢复出口。

### `KernelContext` 和 `TrapFrame` 分别是谁的现场

```text
TrapFrame
= 用户被 trap 时的用户 CPU 现场

KernelContext
= 进入用户前，内核管理流程自己需要恢复的调用现场
```

第一版 `KernelContext` 至少要覆盖普通 RISC-V ABI 要求跨调用保持的状态：

```text
ra
sp
s0～s11
```

如果当前内核运行环境对 `gp`、`tp` 或其他可信状态有明确依赖，也要用设计好的方式恢复；不要写成“随便多存几个寄存器应该够”。

`stvec`、`sscratch`、中断状态等特权运行约定可以由结束路径显式恢复到“内核管理模式”的已知值，不必把所有东西都伪装成普通 ABI 寄存器。

### `KernelContext` 放在哪里

它的生命周期必须覆盖整次用户运行：

```text
run_user 保存
  ↓
用户运行任意长时间
  ↓
exit/fault
  ↓
恢复
```

所以不能把它放在：

- 会被用户栈覆盖的位置；
- 即将清理的 trap 栈片段；
- 某个返回后已经失效的临时 Rust 引用中。

第一版使用固定可信存储即可。

### trap handler 只“决定”，汇编负责“切控制流”

Rust 可以返回类似：

```text
ResumeUser
TerminateProcess
KernelFault
```

有效 `exit`：

1. 检查参数；
2. 把 Process 状态设成 `Exited(code)`；
3. 返回 `TerminateProcess`；
4. Rust handler 自己正常返回汇编；
5. 汇编再恢复 KernelContext。

这样不会在持有 Rust 局部借用、锁 guard 或待析构对象时突然跳走。

当前阶段虽然还没有这些复杂资源，也要先建立正确控制流模型。

### A 次目标输出

用户执行：

```text
exit(7)
```

管理流程重新拿回控制后输出：

```text
kernel resumed: exit=7
```

用户在 `exit(7)` 后面放一个独特失败标记：正确实现永远看不到它。

### A 次验收

- [ ] 有效 exit 不执行用户后续指令。
- [ ] Process 记录 `Exited(7)`。
- [ ] `run_user` 回到它原来的 Rust 调用者，而不是跳进一个陌生 Rust 入口。
- [ ] `KernelContext` 和 `TrapFrame` 的用途能明确区分。
- [ ] 非法 `exit(256)` 返回 -2，用户还能继续执行。

---

## B 次：顺序运行、重置和安全回收

### 为什么“用户已经退出”不代表所有内存马上都能清

exit trap 发生时，CPU 当前正在：

```text
可信 trap 栈
```

上执行汇编/Rust trap 处理。

所以不能在仍使用这块栈时就：

```text
清零 trap 栈
```

正确顺序是：

```text
用户 exit
  ↓
trap handler 返回汇编
  ↓
恢复 KernelContext / 切回管理栈
  ↓
确认已经不再使用用户 trap 栈
  ↓
再清理用户运行资源
```

这是后面所有资源生命周期课程的重要模式：**先脱离资源，再释放资源。**

### 新实例必须完整初始化

顺序运行 A、B 时，每次新实例都重新设置：

- process id / run id；
- Program；
- state；
- user entry；
- 用户寄存器现场；
- user stack 初始状态；
- trap stack 初始状态；
- 上一次退出/错误信息。

不能只改 `state=Ready` 就认为是一个新进程。

### 做 100 次重复实验

连续运行一个很短的程序 100 次：

```text
create/reset
→ run_user
→ exit
→ manager resumes
→ cleanup
```

记录：

- 总完成次数必须正好 100；
- 每次 id 按预期变化；
- 固定栈地址可以复用，但 `sp` 不应该一轮轮向下漂移；
- 结束后没有残留 Running 状态；
- 最终资源计数回到阶段基线。

### 为什么这仍然不是“调度”

如果执行顺序是：

```text
A 完全结束
  ↓
B 才开始
```

这叫顺序运行，不是两个任务轮流使用 CPU。

第 12 课才会第一次实现主动切换。

## 常见问题

| 现象 | 先检查 |
| --- | --- |
| `exit(7)` 后又执行用户代码 | 是否错误走了 ResumeUser / `sret` 路径 |
| `exit(256)` 也终止 | 参数检查是否在状态修改前完成 |
| 第二次运行立刻退出 | 新实例是否残留旧 sepc/a0/state |
| 每轮用户/trap sp 都降低 | 是否积累旧 frame，没有正确回到栈顶 |
| 退出瞬间内核 fault | 是否过早清理仍在使用的 trap 栈，或 KernelContext 不完整 |
| Rust 管理函数返回地址异常 | `run_user` 是否真正按 ABI 成对保存/恢复 |

## 最终理解验收

不看正文回答：

1. 有效 `exit` 为什么不需要“为了返回用户”修改 `sepc`？
2. 为什么非法 exit 参数仍要正常返回用户？
3. `KernelContext` 与用户 `TrapFrame` 分别恢复谁？
4. 为什么安全的设计是“trap handler 先正常返回汇编，再由汇编恢复内核继续点”？
5. 为什么必须先切回管理栈，才能清理 trap 栈？
6. A 完全结束后才运行 B，为什么还不叫调度？

## 下一课为什么自然出现

现在正常用户程序能：

```text
系统调用
→ 返回
→ exit
→ 内核继续
```

但如果用户执行非法指令呢？它不应该让整个内核也一起“死掉”，更不能被伪装成一次正常 exit。

下一课：[第 10 课：用户程序出错，不等于内核出错](10-user-errors.md)。把有效/非法 exit 与 100 次重复运行的实际结果写进 [进度记录](progress.md)。