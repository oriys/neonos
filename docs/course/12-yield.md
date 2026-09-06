# 第 12 课：你运行一会儿，再换我

状态：待开始。前置：[第 11 课](11-scheduling.md) 和第二阶段实现完成。分 A、B 两次，每次 45～60 分钟。

## 这课只做“主动切换”

第 11 课已经会从 Ready 任务中选择谁，但 neonos 还不会真的切换 CPU 现场。

本课先把“时钟”排除掉，只允许用户自己请求：

```text
yield()
```

目标：

```text
A 运行到 yield
→ 保存 A
→ B 从自己的位置运行
→ B yield
→ A 从原 yield 后继续
```

如果 A 永远不 yield，它仍能独占 CPU。这是本课故意保留的限制，第 13～14 课再解决。

## A 次：从一个 Process 槽变成多个可恢复任务

### 第一步：先扩展资源，不急着写 yield

设置教学上限，例如最多 4 个任务。每个任务必须拥有彼此独立的：

```text
process/task id
state
user stack
trap stack
persistent user context / TrapFrame
调度信息
```

Program 描述可以共享；用户可写栈和运行现场不能共用。

当前还没有物理页分配器，因此可以按固定槽位静态预留这些栈。重点是边界和所有权明确，不追求动态分配。

### 第二步：建立 Ready queue

计划创建：

```text
src/scheduler.rs
```

第一版用固定容量环形队列/数组即可。

写出并一直检查几个不变量：

```text
只有 Ready 任务能在 ready queue
同一 task 最多出现一次
最多一个 Running
Exited/Faulted 不再入队
队列长度 <= 固定容量
```

入队和出队集中实现，不要让 syscall、trap handler、scheduler 三处各自偷偷修改队列。

### 第三步：把第 09 课的 `run_user` 发展成“派发一次”

第二阶段：

```text
manager → run_user → exit/fault → manager
```

现在变成：

```text
scheduler loop
  ↓ choose Ready
  ↓ dispatch(task)
用户运行
  ↓ yield/exit/fault
汇编恢复管理 KernelContext
  ↓
dispatch 返回 outcome
  ↓
scheduler loop 处理状态，再选下一项
```

关键要求：**每次切换都回到同一个中央调度循环，不递归调用新的 scheduler。**

否则：

```text
yield → scheduler → user → yield → scheduler → ...
```

如果每层都留着旧 Rust/汇编栈帧，最终会把管理栈耗尽。

### 第四步：用户现场必须持久化

用户 trap 时，最初 TrapFrame 位于该任务的 trap 栈。

如果要离开这个任务并未来再回来，必须在 trap 栈被复用/重置前，把恢复所需现场放到该任务的**持久存储**。

不要保存：

```text
&mut TrapFrame 指向旧 trap 栈
```

然后跨调度长期持有这个 Rust 引用。

可以：

```text
复制 TrapFrame 值到 process/task context
```

或设计一个生命周期明确、不会被复用的固定 frame 槽。无论哪种方案，都要能说明所有权。

### A 次验收

- [ ] 最多 4 个任务的用户栈/trap 栈/现场互不重叠。
- [ ] Ready queue 不允许重复入队。
- [ ] scheduler 只存在一个中央循环，不递归增长管理栈。
- [ ] 任务现场有持久位置，不依赖旧 trap 栈引用。
- [ ] 此时 timer 仍关闭。

---

## B 次：接入 `yield`

### 先定义 syscall

延续课程协议：

```text
a7 = 3  → yield
无参数
恢复本任务时 a0 = 0
```

`yield` 是一次已处理的 U-mode `ecall`，所以在保存待恢复现场前：

```text
sepc += 4
a0 = 0
```

这样未来重新运行时，从 `yield` 后一条指令继续。

### Running → Ready → 队尾

用户 A 调 `yield`：

```text
A Running
  ↓ 保存现场
A Ready
  ↓ enqueue tail
回 scheduler
```

scheduler 再取队首。

如果只有 A 一个任务：

```text
A yield
→ A 进队尾
→ scheduler 又选 A
→ A 从 yield 后继续
```

不能因为“还是同一个任务”就重新从 program entry 初始化。

### 双任务确定性实验

A、B 各执行三轮：

```text
输出自己标记
私有计数 += 1
yield
```

初始 Ready 顺序 A、B，且 putchar 本身不触发重新调度时，预测：

```text
ABABAB
```

注意这不是“RR timer”结果，而是用户主动在固定位置让出造成的确定顺序。

每个任务还在：

- 一个按协议应保持的用户寄存器；
- 自己用户栈上的局部位置；

放不同哨兵值。每次恢复后先检查，再继续。

### 终态不再入队

如果 B 提前：

```text
exit
或 fault
```

它变成终态，scheduler 不再把它放回 Ready queue。

资源真正清理由管理栈上的安全点执行，不在仍使用对应 trap 栈时清。

## 做一个“栈不增长”压力实验

让两个任务累计执行大量 yield，例如 1000 次切换，并记录：

```text
scheduler/management sp 的基线范围
每个 task trap stack 使用后是否回到预期顶部
最终完成计数
```

目标不是性能，而是证明没有：

```text
每 yield 一次就永久多留一层旧栈帧
```

## 常见问题

| 现象 | 先检查 |
| --- | --- |
| A 每次从开头开始 | 是否重新初始化入口，而不是恢复 persistent context |
| Ready queue 里 A 出现两次 | 是否有多个代码路径都负责 enqueue |
| B 使用了 A 的计数/栈数据 | user stack 或 persistent context 是否共用 |
| yield 返回后马上再次 yield | 保存的 `sepc` 是否仍指向旧 ecall |
| 运行越久管理 sp 越来越低 | scheduler 是否递归/旧栈帧没有真正退出 |
| 退出任务又被运行 | terminal state 是否仍被错误入队 |

## 最终验收

### 运行验收

- [ ] A/B 在确定条件下得到 `ABABAB`，各自计数=3。
- [ ] 每次 yield 后从 ecall 后继续，不从入口重启。
- [ ] 用户寄存器与栈哨兵保持正确。
- [ ] 单任务 yield 后仍能继续自己。
- [ ] 一个任务 exit/fault 后其他任务继续。
- [ ] 队列空、队列满、重复入队有明确结果。
- [ ] 压力切换后管理栈不持续增长。

### 理解验收

不看正文回答：

1. scheduler policy 和 context switch mechanism 分别是什么？
2. 为什么 yield 需要先把 `sepc` 指向 ecall 后面？
3. 为什么不能跨调度保留指向旧 trap 栈的 `&mut TrapFrame`？
4. 为什么所有切换都应该回同一个中央 scheduler loop？
5. 为什么本课仍无法处理一个从不 yield 的死循环任务？
6. Ready queue 的四个核心不变量是什么？

## 下一课为什么自然出现

现在多任务已经能切换，但控制权仍取决于用户是否愿意调用 `yield`。

下一问题变成：

> **即使用户什么都不调用，内核能不能在未来某个时间点自动重新得到 CPU？**

下一课：[第 13 课：给内核接上时钟](13-timer.md)。把双任务轨迹、哨兵结果和压力切换的栈范围写进 [进度记录](progress.md)。