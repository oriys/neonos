# 第六阶段：线程、锁与同步

状态：课程已安排，学习待开始。入口：[总路线](README.md) · [进度记录](progress.md)。

目标：从确定性逻辑 race 出发，把 scheduler 对象从 Process 拆成 Thread；再实现 user-visible blocking mutex、condition variable、semaphore，并用 wait-for graph 证明/修复 deadlock。

阅读 OSTEP 第 26～33 章。宿主机实验用于先理解并发语义，neonos 内核实验用于验证单 hart、timer-preemptive user Threads 与 kernel state machine。

## 前置

先完成第五阶段：

- fork/exec/wait/pipe/shell 稳定；
- Blocked syscall 采用原 ecall retry；
- user-copy/fd/ref 生命周期清楚；
- S-mode kernel 路径仍不在任意 Rust stack 中被 scheduler 抢占；
- UART idle/timer path 可运行。

## 14 次学习安排

| 次数 | 课程 | 当次问题 | 当次成果 |
| --- | --- | --- | --- |
| 1 | [29 race](29-races.md) | Atomic 访问为什么仍可能逻辑丢更新 | Barrier 确定性 load/store=1，fetch_add=2，无 Rust UB |
| 2 | [30A Thread 模型](30-threads.md) | Process 与调度实体怎么拆 | scheduler 按 TID，旧单线程 Process 兼容 |
| 3 | [30B create/return](30-threads.md) | 新 Thread 的入口/stack/return 在哪里 | user return stub、独立 stack/trap context |
| 4 | [30C exit/join](30-threads.md) | Thread 与 Process 生命周期怎么区分 | join/reap、last-thread→Process Zombie、fault policy |
| 5 | [31A atomic/invariant](31-atomics.md) | 单字段原子为什么不等于复合事务 | Acquire/Release 最小直觉、统一临界区 |
| 6 | [31B 单核 kernel 同步](31-atomics.md) | spin/关中断/多核锁为什么不同 | irq save/restore、禁止 irq-off 等另一个 Thread |
| 7 | [32A blocking mutex](32-mutex.md) | 拿不到 mutex 怎样不 spin | FIFO wait + direct handoff + `MutexGranted` |
| 8 | [32B 边界](32-mutex.md) | destroy/fault/priority inversion 怎么处理 | generation、pending refs、Process termination policy |
| 9 | [33A condvar](33-condvar.md) | 如何原子 release+wait 并重新拿锁 | Waiting→Reacquiring→Granted 状态机 |
| 10 | [33B bounded queue](33-condvar.md) | predicate 为什么必须 while 重查 | 1P1C/2P1C 数据守恒 |
| 11 | [34A semaphore](34-semaphore.md) | permit 如何 direct grant 不重复计数 | `SemGranted`、count/in-use/pending 守恒 |
| 12 | [34B semaphore queue](34-semaphore.md) | empty/full/mutex 怎样组合 | 不持 queue mutex 等 permit，和 condvar 对照 |
| 13 | [35A deadlock](35-deadlocks.md) | timeout 怎样升级成等待环证据 | Coffman 条件、wait-for graph、统一 lock order |
| 14 | [35B event model](35-deadlocks.md) | blocking Thread 与 event state machine 如何比较 | 同输入结果一致，长 callback/同步边界明确 |

## Thread / Process 不变量

### Process owns

```text
PID
AddressSpace
fd table
parent/children
Thread table
sync object table
```

### Thread owns

```text
TID
user/trap stack
persistent user context
Ready/Running/Blocked/Exited state
MLFQ accounting
pending syscall/wait state
```

同 Process Thread 共享 AddressSpace，因此能访问彼此 user stack；独立 stack 不是 security isolation。

同 Process Thread switch 可保持 `satp` root，但必须换 current TID/context/`sscratch`。跨 Process 继续按 ASID=0 做 root switch + full local `sfence.vma`。

## 多线程后的旧接口限制

第一版为了保持可证明语义：

```text
fork/exec
```

只允许当前 Process 只有一个 live Thread、其他 Thread 已 join、无 pending syscall、sync objects 已销毁，否则：

```text
-9
```

多线程期间不提供用户可调用的 arbitrary unmap/remap；一次 user-copy 仍在单 hart/nonpreemptive kernel 下拥有稳定 mapping。

共享 fd table 中，如果一个 Thread Blocked 在 read/write，相关 fd 标记 in-use；其他 Thread close/dup2 覆盖该 fd 返回 -9，避免 retry ecall 操作到新对象。

## kernel 内部同步边界

当前单 hart：

```text
短 metadata 临界区
→ save interrupt state
→ disable relevant S interrupt/preemption entry
→ bounded state mutation
→ restore old state
```

临界区禁止：

- sleep/Blocked；
- scheduler switch；
- 等另一个 Thread 才会改变的条件；
- 慢/无界 UART 打印；
- 无界 spin。

这不是 SMP lock。未来多 hart 还需要真正跨 hart atomic lock + memory-ordering + TLB/interrupt coordination。

## blocking sync syscall 的 pending completion

### mutex

```text
Waiting
→ unlock direct handoff owner=waiter
→ MutexGranted
→ 原 ecall 再进 dispatcher
→ consume pending, return success
```

### condvar

```text
CondWaiting
→ signal
→ ReacquiringMutex
→ mutex handoff
→ CondWaitGranted
→ return user while owning mutex
```

### semaphore

```text
SemWaiting
→ post direct grant
→ SemGranted
→ return without second count--
```

这些调用已经产生了“登记/释放/permit handoff”等副作用，所以不能像第 25 课普通 wait 一样无脑从头执行普通逻辑。

## user-visible sync ABI

原 1～12 保留：

| a7 | 接口 |
| ---: | --- |
| 13 | `thread_create(entry,arg)` |
| 14 | `thread_exit(code)` |
| 15 | `thread_join(tid,status_ptr)` |
| 16 | `mutex_create()` |
| 17 | `mutex_lock(handle)` |
| 18 | `mutex_unlock(handle)` |
| 19 | `cond_create(mutex_handle)` |
| 20 | `cond_wait(handle)` |
| 21 | `cond_signal(handle)` |
| 22 | `sem_create(initial,max)` |
| 23 | `sem_wait(handle)` |
| 24 | `sem_post(handle)` |
| 25 | `sync_destroy(handle)` |

新增错误：

```text
-9  busy / state not allowed
-10 invalid TID/handle / cross-process / non-owner
```

## Thread fault/exit policy

- `thread_exit` 只结束当前 Thread；last live Thread → Process Zombie。
- Process `exit` 结束所有 Threads。
- user Thread fault：教学版结束整个 Process，因为 shared AddressSpace 可能已经破坏共享 invariant。
- Thread 持 mutex 主动 exit：也按 invariant violation 结束 Process，不自动 unlock 后继续。
- Process cleanup 必须取消所有 wait queues/pending grants、fd pins、sync objects，再释放 AddressSpace。

## stage tests

计划：

```text
experiments/races.rs
experiments/deadlock.py
tests/concurrency.sh
```

必须区分：

- expected logical-race 负例；
- expected deadlock（以 wait-for cycle 为通过证据）；
- fixed positive completion。

正例多轮后检查：

```text
Thread slots
Process slots
sync object count
wait queues
pending grants
fd pins
pipe refs
frames
```

全部回到定义的 baseline。

## 阶段总验收

- [ ] race 实验确定性且无 Rust UB。
- [ ] Process shared resource / Thread execution state 分离。
- [ ] user return stub / thread_exit / join / last-thread process completion 正确。
- [ ] atomic operation 和 compound invariant 能区分。
- [ ] 单 hart irq masking 与 user spin/SMP lock 边界清楚。
- [ ] mutex direct handoff 不重复 acquire。
- [ ] condvar 无 lost wakeup，返回前持 mutex，user while 重查 predicate。
- [ ] semaphore permit 不重复、不泄漏，binary semaphore 不冒充 mutex。
- [ ] deadlock 有 wait-for cycle，lock ordering 修复只在覆盖范围内成立。
- [ ] deadlock/starvation/livelock/event-driven 区分明确。
- [ ] 旧 shell/pipe/process/memory/scheduling tests 全回归。

通过后进入 [第七阶段：块设备与文件系统](stage-07.md)。