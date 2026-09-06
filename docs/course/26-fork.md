# 第 26 课：一次调用，父子两条执行路线

状态：待开始。前置：[第 25 课](25-wait.md) 验收完成。分 A、B 两次，每次 45～60 分钟。

## `fork` 最神奇的地方是什么

用户代码只调用一次：

```text
fork()
```

但成功后有两个 Process 从**同一条 `ecall` 后面**继续：

```text
parent: a0 = child_pid
child : a0 = 0
```

它们拥有相同的初始用户内存内容，却从这一刻起彼此独立修改。

本课实现最简单的 eager copy：所有 user pages 立即复制，不做 copy-on-write。

阅读 OSTEP 第 5 章 fork 行为。

---

## A 次：先画“复制什么、重建什么、共享什么”

### 复制：用户可见状态的快照

child 得到 parent 当前用户执行状态的快照：

```text
user code pages
user data/bss pages
user stack pages
用户整数 TrapFrame/context
虚拟地址布局和 leaf permissions
```

但返回值和 PC 要按 fork 语义修改。

### 重建：属于执行实例/内核可信资源的东西

不能直接 memcpy parent Process：

```text
child PID/generation       → 新建
child AddressSpace root    → 新建
child page-table frames    → 新建
child user physical frames → 新建并复制内容
child trap stack           → 独立可信资源
child scheduler state      → 按“新任务”策略初始化
child KernelContext        → 未来 dispatch 时重新建立
parent relation            → 指向当前 parent
```

尤其不能复制：

```text
指向 parent trap stack 的引用
parent 当前 KernelContext 指针
parent Ready-queue 节点状态
```

### 共享/借用：kernel mapping targets

像第 21 课：

```text
kernel code/data
UART
scheduler/trap entry
allocator RAM mapping
```

可以出现在 child root 中，但只是 borrowed kernel mapping target。

fork 不复制这些物理页，也不让 child 获得它们的所有权。

### guard/unmapped 必须保持 unmapped

复制 address space 时不要写：

```text
for 每个 user VA 范围：
    都 alloc 一页
```

那会把 stack guard 也偷偷补成有效页。

应遍历 parent **实际拥有的 user leaf mappings**：

```text
mapped user leaf → alloc child frame + copy
unmapped guard    → 保持 unmapped
```

权限逐页保持：RX 仍 RX，RW 仍 RW；不因为复制方便而全部改 RW。

---

## child 必须先作为不可见 candidate 构造

和 exec 一样，fork 需要失败原子性。

概念流程：

```text
parent 正在 fork trap
  ↓
reserve 一个 Building slot（调度器不可见）
  ↓
构建 ChildCandidate
  ↓
全部成功
  ↓ publish
child Ready
```

`Building` 不是 Ready，也不能被 wait/调度器当成正常 child 观察到。

可以提前分配一个内部 generation/PID 候选；即使失败造成 PID 序号跳号也没关系，但**不能产生一个用户可见、随后又莫名消失的半成品 child**。

## 用户内存复制必须在稳定快照下进行

当前阶段：

```text
单 hart
同一个 Process 只有一个用户执行线程
S-mode fork handler 不被调度抢占
没有用户 unmap API
```

所以 parent user page 在 copy 期间稳定。

第 30 课加入多线程后，这条前提不再成立；第六阶段会明确限制：多线程 Process 暂不允许 fork/exec。

## 构建 child AddressSpace

对 parent 每个 owned user leaf：

```text
alloc zeroed child frame
→ 通过 trusted kernel mapping 复制 4 KiB 内容
→ child map 同一个 VA
→ 保持 parent leaf permissions
```

对 user code 即使是 RX，本课程也选择 eager physical copy，方便建立“父子所有 user frames 都独立拥有”的简单模型。

以后 COW 才会改成共享只读 + 写时复制。

### 指令内容复制后同步

child code frame 是刚被 CPU 写入、以后会执行的指令，所以完成复制后执行本 hart 需要的 `fence.i`/指令同步，再让 child 可运行。

---

## fork 返回现场必须从同一个“post-ecall”位置分叉

先计算：

```text
resume_sepc = parent_saved_sepc + 4
```

只因为当前确定是已识别的 32-bit U-mode `ecall`。

### child context

从 parent **用户 TrapFrame 值**复制出一份 child frame，然后改：

```text
child.sepc = resume_sepc
child.a0   = 0
child.sp   = 与 parent 相同的虚拟地址值
```

为什么 child `sp` 数值可以相同？

因为 child root 中相同 user stack VA 映射的是复制后的 child physical frame。

### parent context

只有 child 所有资源都构造成功、准备 publish 时，才提交：

```text
parent.sepc = resume_sepc
parent.a0   = child_pid
```

不要在构建刚开始时先改 parent 返回值，然后中途失败再费力回滚。

### publish 的最后几步

建议顺序：

1. ChildCandidate 所有 frame/context 完整；
2. parent/child relation 写入；
3. child slot 从 Building → Ready；
4. enqueue child 一次；
5. 最后设置 parent fork 成功返回值；
6. parent 继续走 `ResumeUser`。

实现可以在同一个 S-mode 不可抢占临界区提交，确保调度器不会看到“Ready 但 relation/现场还没完成”的 child。

### A 次验收

- [ ] child root/table/user frames 都独立拥有。
- [ ] guard page 保持 unmapped，leaf permissions 保持。
- [ ] child user context 从 fork 后继续，`a0=0`。
- [ ] parent 从同一 fork 后继续，`a0=child_pid`。
- [ ] child 只有完整构造后才 Ready/可见。

---

## B 次：失败、生命周期和真实 parent/wait 联调

## 失败路径必须只影响 parent 的“返回值”

例如 OutOfFrames：

```text
fork()
→ child candidate 构造失败
→ 回滚所有 child owned frames/table/slot reservation
→ parent.a0 = -4
→ parent.sepc += 4
→ parent 继续
```

parent：

```text
AddressSpace 内容不变
user stack/data 不变
children relation 不增加
scheduler state 不变
```

### 小页池故意失败

让 child copy 到一半时耗尽：

```text
已经复制 code
已经复制几个 data/stack page
→ 下一 frame 分配失败
```

验证：

```text
所有本次 child frame 回收
Building slot 清理
parent page count/内容不变
Ready queue 没有 child
```

这比只测试“开始就没空槽”更能证明 rollback。

## capacity full

Process slot 已满：

```text
fork → -4
```

不要：

- 分配一堆 frame 后才发现没有 slot；
- 返回一个 child PID 但 child 永远不会运行。

先预留基础 slot/capacity，再做昂贵复制。

## 父子独立数据实验

fork 后：

```text
parent: *same_user_va = P
child : *same_user_va = C
```

经过 yield/timer 多轮后：

```text
parent 仍 P
child 仍 C
```

software walk 也验证：

```text
same VA → different PPN
```

不要只看用户值相同，因为复制刚完成时两边本来就一样。

## parent `wait` 真正替换测试夹具

程序：

```text
pid = fork()

if pid == 0:
    child work
    exit(7)
else:
    wait(pid, &status)
    check status == 7
```

调度器可以先运行 parent，也可以先运行 child。

测试不能写死：

```text
一定 parent 先打印某行
```

只约束 fork/wait 的因果：parent wait 成功一定发生在 child completion 可被 reap 之后。

## 生命周期压力

重复：

```text
fork
→ child exit/fault
→ parent wait/reap
```

多轮检查：

```text
Process slot 回到基线
free_frame_count 回到基线
Ready/Blocked queue 无残留
Zombie 数回到 0
```

还要测试：

- parent 提前 exit → child 交 KernelManager；
- child fault → parent wait 得到 fault status；
- parent status_ptr 错误 → Zombie 不被吞。

## 为第 27 课提前写下 fd 规则

当前 fork 时还没有 pipe/file fd table。

第 27 课引入以后，fork 要扩展为：

```text
复制 fd table entries
→ 对共享 endpoint/open-object 增加引用
→ 不复制 pipe ring buffer 本体
```

如果 fork 之后阶段失败，新增 fd 引用也必须回滚。

本课先把这个未来扩展写在设计注释里，不提前实现不存在的 fd 子系统。

## 常见问题

| 现象 | 先检查 |
| --- | --- |
| child 无限 fork | child.sepc 是否仍停在原 fork ecall |
| parent/child 返回值一样 | 是否改了同一份 TrapFrame，或 publish 顺序错误 |
| child 修改数据影响 parent | 是否只复制 page table，没有复制 user physical frames |
| fork 失败后 free frames 变少 | candidate rollback 是否覆盖所有已复制 page/table |
| fork 失败却能 wait 到 child | Building slot 是否提前对外可见 |
| child trap 用 parent 栈 | 是否复制了 trap-stack pointer/KernelContext 而不是重建 |

## 最终验收

### fork 语义

- [ ] parent `a0=child_pid`，child `a0=0`。
- [ ] 二者 `sepc` 都从 fork ecall 后继续。
- [ ] user VA/权限相同，但 owned PPN 独立。
- [ ] parent/child 后续写入互不影响。

### 发布/失败

- [ ] child 在完整构造前不可调度/不可 wait 观察。
- [ ] 中途 OutOfFrames rollback 无 frame/slot/queue 泄漏。
- [ ] capacity full 无半成品 PID。
- [ ] parent 失败时只得到错误码，原用户状态保持。

### 生命周期

- [ ] fork→exit/fault→wait/reap 多轮资源回到基线。
- [ ] parent 早退 orphan 最终被 manager 回收。
- [ ] wait bad-pointer 规则继续成立。

### 理解验收

不看正文回答：

1. fork 哪些内容是“复制”，哪些必须“重建”？
2. 为什么 parent/child user `sp` 数值可以相同却不共享栈内容？
3. 为什么 child 必须到最后才 publish Ready？
4. fork 中途失败时 parent 哪些东西绝不能被改坏？
5. 为什么本课 eager copy code page，而 kernel mapping target 仍共享？
6. 多线程出现以后，为什么当前 fork 快照前提会失效？

## 下一课为什么自然出现

现在 parent/child 能独立执行和 wait，但进程之间还没有一个“字节流通道”。

下一课第一次引入 fd 和阻塞 I/O：

```text
pipe
→ read/write
→ empty/full 阻塞
→ close 改变 EOF/错误语义
```

进入 [第 27 课：用管道连接两个进程](27-pipes.md)。把 fork 成功/中途失败/生命周期资源基线写进 [进度记录](progress.md)。