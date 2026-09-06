# 第 32 课：拿不到锁，就先让 CPU 做别的事

状态：待开始。前置：[第 31 课](31-atomics.md) 验收完成。分 A、B 两次，每次 45～60 分钟。

## 本课实现的是“用户可见的睡眠 mutex”

用户 Thread 调：

```text
mutex_lock(handle)
```

如果锁被别人持有，不在 kernel 里 spin，而是：

```text
Thread → Blocked
→ scheduler 运行别的 Thread
→ owner unlock
→ waiter Ready
```

这把 mutex 由 kernel 对象表管理；kernel 自己保护 scheduler/object metadata 仍使用第 31 课的短 irq-off 临界区，不在 interrupt handler 里调用这个会 Blocked 的用户 mutex API。

---

## A 次：owner + FIFO wait queue + direct handoff

## 对象表

每个 Process 有固定数量 sync objects。

mutex 至少保存：

```text
object generation / handle
owner: Option<TID>
FIFO waiters: [TID...]
bound_condvar_refs
```

handle 不能只等于对象槽位；generation 防止旧 handle 命中新对象。

对象只属于创建它的 Process。跨 Process/无效 handle：

```text
-10
```

## 无竞争 lock

在一个短 kernel metadata 临界区：

```text
owner == None
→ owner = current_tid
→ return 0
```

### 非递归

```text
owner == current_tid
→ -9
```

不要让同一个 Thread 第二次 lock 后永远等自己。

本课程 mutex 是明确的 non-recursive mutex。

## 有竞争时不能“先回 scheduler 再登记”

正确：

```text
在同一个 irq-off / scheduler-safe 临界区：
  再检查 owner
  → enqueue current TID exactly once
  → pending = MutexWaiting(handle)
  → Thread = Blocked(Mutex(handle))
→ 回 scheduler
```

这样不会在：

```text
检查 locked
和
真正进入 wait queue
```

之间漏掉一次 unlock。

Blocked Thread 不在 Ready queue。

## 为什么不能只靠“wake 后重新执行 lock”

本课程采用 **direct handoff**：unlock 有 waiter 时，不先把 mutex 变成 free 再让大家抢。

而是：

```text
unlock by owner
→ pop FIFO waiter W
→ owner = W
→ W.pending = MutexGranted(handle)
→ W Blocked→Ready
```

此时 W 在真正运行前就已经成为 owner。

如果 W 被调度后只是把原 `mutex_lock` ecall 从头执行普通逻辑：

```text
owner == current_tid
→ 看起来像递归 lock
→ 错误 -9
```

所以 pending syscall 必须有“完成阶段”。

## 被唤醒后的原 ecall 如何完成

Thread 的 persistent context 仍让 `sepc` 指向原 `mutex_lock` ecall。

W 被 scheduler 恢复后重新执行这条 ecall，dispatcher 先检查：

```text
pending == MutexGranted(same handle)
AND owner == current_tid
```

成立：

```text
consume pending
→ a0=0
→ sepc += 4
→ return user
```

**不要再次普通 acquire，也不要再次入队。**

如果 pending handle 和实际 syscall 参数不匹配，说明状态机损坏，应该报 kernel invariant failure，而不是悄悄继续。

## unlock

只有：

```text
owner == current_tid
```

才能成功。

非 owner：

```text
-10
```

成功时：

```text
if waiters not empty:
    direct handoff to FIFO head
else:
    owner = None
```

unlock 本身不 Blocked。

### A 次共享计数实验

两个用户 Thread 各循环 N 次：

```text
mutex_lock
load counter
counter += 1
store counter
mutex_unlock
```

最终：

```text
counter == 2N
```

同时记录：

- 每次 owner；
- wait queue；
- direct handoff；
- pending grant consume。

不要只看最终 counter；如果队列重复入 TID，也可能偶然得到正确数值。

---

## B 次：抢占、销毁、故障和优先级边界

## 在 holder 临界区中故意被 timer preempt

让 A：

```text
lock
做一段有限计算
→ timer preempt
```

B：

```text
mutex_lock
→ Blocked
```

scheduler 未来再运行 A：

```text
unlock
→ handoff B
```

验证 B 没有在 kernel spin 消耗时间片。

### MLFQ 用量规则

Thread 因 mutex Blocked：

```text
不继续消耗 user CPU 时间
```

但**不重置已经使用的 allotment**。

唤醒也不因为“等过锁”自动回最高优先级；只有阶段既定的 periodic boost 等规则改变优先级。

## priority inversion 只观察，不假装已经解决

构造：

```text
低优先级 L 持 mutex
高优先级 H 等 mutex
中优先级 M 持续 Ready
```

可能观察 H 被 L 间接阻塞。

本课不实现 priority inheritance/ceiling，只要求能解释：

```text
FIFO mutex fairness
≠ priority inversion solution
```

## `sync_destroy`

mutex 只有在：

```text
owner == None
waiters empty
没有 pending grant/reference
bound_condvar_refs == 0
```

才能销毁。

否则：

```text
-9
```

销毁后 generation 增加，旧 handle → -10。

## Thread 在持锁时退出/故障

本阶段政策已经在第 30 课定义：

```text
Thread fault → terminate whole Process
```

另外，如果用户主动 `thread_exit` 时仍拥有 mutex，本教学版也不“自动 unlock 然后让其他线程继续使用可能处于中间状态的数据”。

选择：

```text
报告 invariant violation
→ terminate whole Process
```

Process cleanup：

- 取消所有 mutex waiters/pending grants；
- 不再调度任何 Thread；
- 销毁 Process-owned sync table；
- 然后按 Process termination 流程回收。

因为整个 Process 都结束，不需要把 handoff 中的 owner 再恢复成一个可继续使用的状态。

## 容量/错误场景

测试：

- object table full；
- invalid generation handle；
- cross-process handle；
- recursive lock；
- non-owner unlock；
- destroy while owned；
- destroy with waiter；
- repeated create/destroy 后旧 handle 失效。

## 常见问题

| 现象 | 先检查 |
| --- | --- |
| waiter 醒来立刻得到 recursive-lock -9 | 是否缺 `MutexGranted` completion phase |
| 同一 waiter 出现两次 | Blocked retry 是否重复 enqueue |
| unlock 后别人插队抢到 | 是否先 owner=None 再 wake，而不是 direct handoff |
| B 等锁时 CPU 一直满载 | 是否在 kernel/user 错误 spin，而没 Blocked |
| destroy 后旧 handle 又命中新对象 | generation 是否验证 |
| holder thread_exit 后其他线程继续使用半成品数据 | 是否违反“kill whole process”阶段政策 |

## 最终验收

### 锁状态机

- [ ] free lock acquire 正确；recursive lock -9。
- [ ] contention 原子登记 Blocked，无 lost wakeup。
- [ ] unlock FIFO direct handoff，owner 始终唯一。
- [ ] waiter 通过 pending grant 完成原 ecall，不二次 acquire。
- [ ] non-owner unlock -10。

### 调度/生命周期

- [ ] holder 被 preempt 时 waiter Blocked，holder 未来仍能运行并 unlock。
- [ ] Blocked 不消耗 user CPU，但 MLFQ allotment 不被重置。
- [ ] priority inversion 可重现/解释但不虚假宣称已解决。
- [ ] destroy 只允许无 owner/waiter/pending/binding。
- [ ] owner Thread 退出/故障按课程政策结束 Process 并清理对象。

### 理解验收

不看正文回答：

1. 为什么 contention 登记 wait 必须和 owner 检查处于同一临界区？
2. direct handoff 和“unlock 后大家重新抢”有什么区别？
3. 为什么 direct-handoff waiter 需要 `MutexGranted` pending state？
4. Blocked mutex 为什么优于在 kernel 中 spin？
5. FIFO 为什么不能解决 priority inversion？
6. 为什么持 mutex 的 Thread exit 不自动 unlock 后继续其他线程？

## 下一课为什么自然出现

mutex 只回答：

```text
“我能不能进入临界区？”
```

但生产者/消费者经常需要等待：

```text
“队列不为空”
“队列不满”
```

这种条件不是“锁有没有人拿”。

下一课实现 condition variable：[第 33 课：等待一个条件，而不只是等待一把锁](33-condvar.md)。