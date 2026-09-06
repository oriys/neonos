# 第 30 课：同一个进程里，运行多条执行路线

状态：待开始。前置：[第 29 课](29-races.md) 验收完成。分 A、B、C 三次，每次 45～60 分钟。

## 这课要把“Process”和“调度实体”正式拆开

此前：

```text
一个 Process
≈ 一个 user context
≈ 一个 scheduler task
```

有 thread 以后：

```text
Process
  ├─ shared AddressSpace
  ├─ shared fd table
  ├─ shared parent/children
  ├─ shared sync objects
  │
  ├─ Thread A: 独立 user context / user stack / trap stack / scheduler state
  └─ Thread B: 独立 user context / user stack / trap stack / scheduler state
```

所以从本课开始：

> **Thread 才是 scheduler 真正运行和阻塞的实体；Process 是共享资源与生命周期的容器。**

阅读 OSTEP 第 26～27 章。

---

## A 次：先把 scheduler 从 PID 改成 TID

## Process 保留什么

至少：

```text
PID/generation
AddressSpace root + owned user pages
fd table
parent/children/Zombie process state
thread table
sync object table
```

## Thread 拥有什么

至少：

```text
TID/generation
owner PID
ThreadState: Ready/Running/Blocked/Exited
persistent user context / TrapFrame
user stack mapping + guard
trusted trap stack
scheduler/MLFQ accounting
pending syscall / wait reason
join completion status
```

不要把同一字段同时放 Process 和 Thread，然后靠“记得同步”。

### 先迁移旧单线程程序

每个旧 Process 创建一个：

```text
main thread
```

原有行为必须先全部通过：

```text
shell
fork/exec/wait
pipe
memory isolation
RR/MLFQ
```

只有在“一个 Process 一个 main thread”完全保持兼容后，再实现 `thread_create`。

### Ready queue / current / MLFQ 全改按 TID

```text
Ready queue item = TID
current = current TID
```

调度时：

1. 根据 TID 找 Thread；
2. 再根据 Thread.owner_pid 找 Process/AddressSpace；
3. 若切换到同 Process 的另一个 Thread，root 不变；
4. 若切换到不同 Process，按第 21 课切 `satp` + `sfence.vma`；
5. 无论是否同 Process，都必须更新当前 thread 的 `sscratch`/trusted trap stack/user context。

### 同 Process thread 共享地址空间，不代表互相隔离

Thread A/B 的 stack 虽然映射在不同 user VA，并各自有 guard page，但：

```text
它们都在同一个 AddressSpace，U=1
```

所以恶意/错误的 A 完全可能访问 B 的 user stack。

“独立 thread stack”是执行组织，不是 security boundary。

### 线程 stack slot

为每个 Thread 分配不冲突的 user virtual stack slot，例如：

```text
[guard][stack pages]
[guard][stack pages]
...
```

创建/撤销 mapping 后按当前 ASID=0 规则执行必要 `sfence.vma`。

trusted trap stack 是 kernel U=0 资源；Thread 切换时 `sscratch` 必须跟 TID 一起更新。

### A 次验收

- [ ] 旧单线程 Process 自动拥有 main TID，全部旧测试保持通过。
- [ ] Ready/Blocked/Running 状态已经属于 Thread。
- [ ] 同 Process 切 Thread 不换 root，但换 user/trap context。
- [ ] 跨 Process Thread 切换继续正确换 root。
- [ ] 能解释为什么 thread stack 不提供同进程安全隔离。

---

## B 次：创建、运行和 thread return

### 新 syscall

```text
a7=13 thread_create(entry,arg)
a7=14 thread_exit(code)
```

`thread_create` 成功只返回给 creator：

```text
a0 = new TID
```

新 Thread 从自己的入口第一次被 scheduler dispatch。

## entry 必须验证是当前 Process 的可执行用户地址

检查：

- canonical user VA；
- page-table leaf `U=1,X=1`；
- 地址落在允许的 user executable region；
- 满足当前目标 ISA 的 instruction-address alignment；
- 不是 kernel U=0 地址/MMIO。

不能允许用户传一个 kernel function pointer 作为 thread entry。

## thread_create 也使用 candidate/publish 模式

```text
reserve Building thread slot
→ 分配 user stack pages + guard
→ 分配/准备 trusted trap stack
→ 构造 user context
→ 完成 page-table mapping + sfence
→ 全部成功
→ publish Ready + enqueue
```

失败：

```text
回滚 stack frames/mappings/slot
creator 返回 -4/-2 等明确错误
```

不要先返回 TID，再发现 stack 分配失败。

## 用户线程“函数返回”怎么处理

不能让 thread entry 的普通 `ret` 跳回 kernel 地址。

课程采用 user-space return stub：

```text
new thread context:
  sepc = entry
  a0   = arg
  ra   = user_thread_return_stub
```

entry 按普通用户 ABI 运行。

如果 entry `ret`：

```text
→ user_thread_return_stub
→ 把约定的返回码放 a0
→ thread_exit(a0)
```

`ra` 指向的是**同一 AddressSpace 中 U=1,X=1 的用户代码**，不是 kernel trampoline。

### 两线程先证明“共享”，再制造竞争

先让 A/B 写共享 data page 的不同槽：

```text
shared[0] = A marker
shared[1] = B marker
```

两者都能读取到对方 marker，证明 shared AddressSpace。

同时在各自 user stack 放不同 sentinel，抢占/切换后 sentinel 不串。

然后再用用户态明确的 load/yield/store 或原子分步实验重现第 29 课逻辑竞争。

不要在 kernel Rust 中创建跨调度存活的 `&mut` 指向 user shared memory；仍通过 user-copy/受控 mapping 访问。

---

## C 次：thread_exit / join / Process 生命周期

### `thread_exit` 和 `exit` 不是同一件事

保留：

```text
exit(code)        → 结束整个 Process
thread_exit(code) → 只结束调用 Thread
```

成功的 `thread_exit` 不返回原用户线程。

Thread 完成后先变：

```text
ThreadExited(status)
```

保留 TID/status 供 join；它不再 Ready/Running。

在安全 management stack 上，确认不再使用该 Thread 的 stack 后：

- unmap/free 它的 user stack frames；
- release/重置 trusted trap stack resource；
- 保留最小 join record。

Process 的 root/data/fd 仍被其他 Thread 使用，绝不能因为一个 Thread 退出就销毁。

### 最后一个活 Thread 结束

如果没有任何 live Thread：

```text
Process execution complete
→ 形成 Process Zombie
```

教学规则：正常情况使用最后结束 Thread 的 code 作为 Process exit status。

如果整个 Process 是通过 `exit(code)` 终止，则用 process exit code，并取消所有其他 Threads。

### Thread fault 的阶段政策

一个 Thread 发生用户 fault 时，教学版选择：

```text
terminate entire Process
```

原因：它可能已经破坏同一 AddressSpace 的共享数据/同步不变量。

不要自动“只杀 faulting thread，然后假装共享数据仍安全”。

Process 终止时：

1. 所有 Thread 不再调度；
2. 取消所有 pending wait/sync/I/O registrations；
3. 清理 fd refs；
4. 清理 sync objects/wait queues；
5. 切安全 root；
6. destroy AddressSpace；
7. 形成 Process Zombie。

### `thread_join(tid,status_ptr)`

```text
a7=15
```

只允许 join 同 Process 的 Thread。

课程规则：

- self join → -9；
- 不存在/跨 Process TID → -10；
- 每个 Thread 只允许一个 join waiter；第二个 waiter → -9；
- 已 Exited → `copy_to_user` status 成功后才 reap Thread record；
- bad status_ptr → -3，completion 记录不被吞；
- 还活着 → Blocked(ThreadJoin(tid))，复用第 25 课原 ecall retry 模型。

### 多线程后对旧接口增加限制

本课程暂不设计 POSIX 的复杂多线程 fork/exec 语义。

只有当 Process：

```text
仅剩当前一个 live Thread
其他 Thread 已 join/reap
没有 pending Thread syscall
sync objects 已销毁
```

才允许 `fork` / `exec`。

否则：

```text
-9 busy/not allowed
```

### fd 共享带来的新问题

fd table 现在由同 Process 所有 Threads 共享。

一个 Thread Blocked 在：

```text
read(fd=3)
```

如果另一个 Thread `close(3)` 再重用这个 fd，原 retry ecall 可能突然操作另一个对象。

教学版先增加 `fd in-use/pinned by pending syscall` 标记：

```text
Blocked read/write 持有使用标记
其他 Thread close(fd) / dup2(...,fd) → -9
```

调用完成/取消/Process 终止时清除标记。

以后可以改成真正 open-file-description 引用快照；当前先保持语义可解释。

### user-copy 稳定性

本阶段仍：

```text
单 hart
S-mode kernel 不被 scheduler 抢占
多线程期间不提供用户 unmap/remap syscall
```

所以一次 user-copy 的 mapping 稳定前提仍成立。

Thread stack 的内部 map/unmap 只在 kernel 确认目标 Thread 未运行的安全点进行，并执行需要的 TLB invalidation。

## 常见问题

| 现象 | 先检查 |
| --- | --- |
| 一个 Thread exit 后整个 Process 地址空间没了 | 是否把 Thread lifetime 和 Process lifetime 混了 |
| 同 Process 切换后 trap 落进旧栈 | current TID/sscratch/trap stack 是否同步更新 |
| thread entry ret 跳到 kernel | `ra` 是否指向 U-mode return stub |
| join bad pointer 后目标消失 | 是否 copyout 前 reap |
| blocked read 醒来操作了新 fd | fd in-use pin/close/dup2 busy 规则是否缺失 |
| fork/exec 多线程时产生奇怪快照 | 是否忘了本阶段 -9 限制 |

## 最终验收

### Thread 模型

- [ ] Process shared resource 与 Thread execution state 明确分离。
- [ ] scheduler 全部按 TID；旧单线程场景回归通过。
- [ ] same-Process Thread 共享 data/fd/root，但 user/trap stack/context 独立。
- [ ] cross-Process 继续页表隔离。

### 生命周期

- [ ] thread_create failure 全回滚，成功最后才 Ready。
- [ ] user return stub 能把普通 function return 转成 thread_exit。
- [ ] thread_exit 不销毁仍被别的 Thread 使用的 Process root。
- [ ] join 两种时序、bad pointer、自身/重复/第二 waiter 都有明确结果。
- [ ] 最后 live Thread 结束形成 Process Zombie。
- [ ] user Thread fault 按课程政策结束整个 Process 并清理所有 waiters/resources。

### 兼容

- [ ] multi-thread fork/exec busy 限制生效。
- [ ] pending fd use 防止其他 Thread close/dup2 改对象。
- [ ] 原 shell/pipe/wait/memory/scheduling tests 继续通过。

### 理解验收

不看正文回答：

1. 为什么 Thread 是调度实体，而 Process 更像资源容器？
2. 同进程 thread stack 为什么独立却不安全隔离？
3. 同 Process 切 Thread 为什么不一定换 `satp`，但一定要换 `sscratch`/context？
4. Thread entry 的 `ra` 为什么必须是 user return stub？
5. 一个 Thread exit 为什么不能 free Process root？
6. 为什么一个 Thread fault 当前选择杀整个 Process？
7. 为什么多线程 fork/exec 暂时返回 -9？
8. 为什么 Blocked fd 要加 in-use pin？

## 下一课为什么自然出现

现在真正有多个 Thread 同时共享一个 AddressSpace，之前第 29 课的竞争不再只是宿主机模拟。

下一步先学习：

```text
一个 Atomic 操作能保证什么
一个“复合不变量”为什么还需要临界区
单核 kernel 自旋/关中断为什么有特殊陷阱
```

进入 [第 31 课：原子操作能保护什么](31-atomics.md)。