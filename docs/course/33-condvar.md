# 第 33 课：等待一个条件，而不只是等待一把锁

状态：待开始。前置：[第 32 课](32-mutex.md) 验收完成。分 A、B 两次，每次 45～60 分钟。

## 为什么 mutex 不够

mutex 能保证：

```text
同一时刻只有一个 Thread 修改共享状态
```

但生产者/消费者还需要表达：

```text
queue not empty
queue not full
```

如果消费者拿到 mutex 后发现 queue empty，它不能：

```text
一直拿着 mutex 等 producer
```

否则 producer 永远拿不到 mutex 去改变条件。

condition variable 解决的是：

> **在持有 mutex 检查条件以后，原子地释放 mutex 并进入等待；被通知后重新获得 mutex，再重新检查条件。**

阅读 OSTEP 第 30 章。Rust/其他系统的 Condvar API 用于理解思想；neonos 继续使用自己的 kernel-managed object 和 syscall 状态机。

---

## A 次：把 `cond_wait` 做成明确多阶段状态机

## 本课程的简化设计：Condvar 绑定一个 mutex

```text
cond_create(mutex_handle)
```

创建时：

- mutex 必须属于当前 Process；
- mutex handle generation 有效；
- Condvar 保存绑定 mutex handle/reference；
- mutex 的 `bound_condvar_refs += 1`。

这样 `cond_wait` 不需要每次额外传 mutex handle，减少教学协议复杂度。

这是 neonos 课程 API，不是所有系统 Condvar 必须采用的唯一设计。

## user 正确用法必须是 `while`

```text
mutex_lock(m)
while !predicate:
    cond_wait(c)
// predicate 此刻在 mutex 保护下重新确认
use shared state
mutex_unlock(m)
```

不能写：

```text
if !predicate:
    cond_wait(c)
```

原因不是只为了“兼容 spurious wakeup”这个词，而是更基础：

```text
signal 只说明“条件可能改变了”
→ waiter 真正重新拿到 mutex 前
→ 共享状态还可能再次变化
```

所以返回后永远重新检查 predicate。

## `cond_wait` 的前置条件

当前 Thread 必须：

```text
是绑定 mutex 的 owner
```

否则：

```text
-10
```

无效/cross-process handle 也 -10。

## 最关键的原子步骤：enqueue + release + Blocked

错误：

```text
unlock mutex
→ 还没进 cond wait queue
→ producer 改条件并 signal（队列没人）
→ consumer 才 Blocked
→ 永远等
```

这是经典 lost wakeup。

正确做法在一个短 kernel metadata 临界区内：

```text
1. 再确认 current thread 仍 owner
2. enqueue 到 cond wait queue
3. pending = CondWaiting(cond,mutex)
4. 按 mutex unlock 规则释放 mutex
   - 可能 direct handoff 给 mutex waiter
5. current Thread = Blocked(Condvar(cond))
6. 回 scheduler
```

从其他 Thread 观察时，不存在“mutex 已释放但 waiter 还没登记”的窗口。

## 为什么 cond_wait 不能像简单 wait 那样从头重试

第 25 课 wait 在 Blocked 前没有副作用，所以 wake 后可以整条 ecall 重新执行。

`cond_wait` 不一样：它已经：

```text
释放了 mutex
进入了 cond wait queue
```

如果 wake 后把普通 `cond_wait` 从头执行：

```text
又 enqueue
又 release
```

状态立即损坏。

所以必须记录阶段：

```text
CondWaiting
→ ReacquiringMutex
→ CondWaitGranted / Completed
```

---

## `cond_signal`

本课程规定 signaler 也必须持有绑定 mutex。

这是一条课程 API 政策，方便把“修改 predicate”和“选择 waiter”放在同一个同步规则下；不要把它说成所有 Condvar 实现的宇宙定律。

```text
cond_signal(c)
```

如果 cond queue 空：

```text
return 0
```

**不保存一张未来可用的通知票。**

如果有 waiter W：

```text
pop FIFO cond waiter W
→ W.pending = ReacquiringMutex(c,m)
→ 尝试进入绑定 mutex 的获取流程
```

### mutex 当前为什么通常还被 signaler 持有

因为本课程要求 signal 在持锁状态调用。

所以 W 一般不能立刻获得 mutex，而是：

```text
加入 mutex wait queue
仍 Blocked
```

等 signaler 之后：

```text
mutex_unlock(m)
```

第 32 课 direct handoff 会把 mutex ownership 交给 FIFO waiter。

如果交给的正是 Condvar waiter W：

```text
owner = W
W.pending = CondWaitGranted(c,m)
W Ready
```

### W 最终怎样从原 `cond_wait` 返回

W 被调度后仍重新执行原 `cond_wait` ecall。

dispatcher 先检查：

```text
pending == CondWaitGranted(same cond, same mutex)
AND mutex.owner == W
```

成立：

```text
consume pending
→ a0=0
→ sepc += 4
→ return user
```

此时 `cond_wait` 的 API 保证成立：

> **返回用户时，这个 Thread 已经重新持有绑定 mutex。**

它没有从头再次 enqueue/release。

## 如果 cond waiter 在重新拿 mutex 期间被 destroy 怎么办

`sync_destroy` 必须把这些都算作引用：

```text
cond wait queue 非空
ReacquiringMutex pending
CondWaitGranted 尚未消费
```

任意存在：

```text
-9
```

mutex 也因为 cond binding/pending waiter 不能被销毁。

---

## B 次：用 bounded queue 证明条件变量语义

共享 user data：

```text
capacity = 4
buffer[4]
head
tail
len
```

所有这些字段都由**同一个 mutex**保护。

两个 Condvar：

```text
not_empty
not_full
```

## producer

```text
lock(m)
while len == 4:
    cond_wait(not_full)

push(item)
cond_signal(not_empty)
unlock(m)
```

## consumer

```text
lock(m)
while len == 0:
    cond_wait(not_empty)

item = pop()
cond_signal(not_full)
unlock(m)
```

### 为什么 signal 在修改 predicate 后

producer：

```text
先 push
→ len 从 0 变 1
→ 再 signal(not_empty)
```

如果先 signal 再修改状态，waiter 即使被唤醒，重新拿锁后也可能仍看不到预期 predicate 变化。

### 固定总量，避免“测试自己永远等”

例如：

```text
producer 产生 100 个编号 item
consumer 明确消费 100 个
```

不要让 consumer 写：

```text
while true:
    等下一项
```

然后靠 QEMU timeout 判断“好像完成”。

每个 item 唯一编号，检查：

```text
0..99 每个恰好一次
无丢失
无重复
单 producer 内部顺序保持
0 <= len <= 4
```

### 再做两个 producer

例如：

```text
P1: 0..49
P2: 1000..1049
```

consumer 总共取 100 项。

不要求两个 producer 的全局输出顺序固定，但每个 producer 自己的序列顺序要保持，且集合完整。

## 故意制造“通知≠条件成立”的场景

在不破坏对象状态的测试模式中，让 signal 发生时 predicate 对某个 waiter 最终仍不满足，或让另一个 Thread 在 waiter reacquire 前先消费掉状态。

waiter 返回 user 后：

```text
while 再检查
→ 如果条件不成立，再 cond_wait
```

验证不会因为“一次 signal”就错误读取空队列。

当前 kernel 可以不主动制造真正随机 spurious wakeup；**while 的必要性仍然成立。**

## 常见问题

| 现象 | 先检查 |
| --- | --- |
| producer 改完状态但 consumer 永远睡 | enqueue+unlock+Blocked 是否原子，是否 lost wakeup |
| waiter 醒来再次把自己加入 cond queue | 是否缺 pending phase，错误从头执行 wait |
| cond_wait 返回后 mutex owner 不是自己 | ReacquiringMutex / direct handoff 是否完整 |
| consumer 醒后读空 | user 是否用 if 而不是 while |
| mutex destroy 总失败 | 是否还有 cond binding/reacquiring/pending ref |
| signal 早于 waiter 后未来 waiter 立即通过 | 是否错误把 condvar 当 semaphore 存通知 |

## 最终验收

### Condvar 状态机

- [ ] wait 只允许 mutex owner 调用。
- [ ] enqueue cond waiter + release mutex + Blocked 无 lost-wakeup 窗口。
- [ ] signal 不存未来 notification。
- [ ] waiter 经 `Waiting → Reacquiring → Granted`，返回时已持 mutex。
- [ ] 原 cond_wait ecall 只完成一次，不重复 enqueue/release。
- [ ] destroy 拒绝所有 queue/pending/binding 引用。

### bounded queue

- [ ] capacity 1 和 4 都通过。
- [ ] producer-first / consumer-first 都可结束。
- [ ] 1P1C / 2P1C 数据集合完整、不重复。
- [ ] len 始终在 0..capacity。
- [ ] user 使用 while 重新检查 predicate。

### 理解验收

不看正文回答：

1. mutex 和 condvar 分别解决什么问题？
2. 为什么 cond_wait 必须把“登记等待+释放锁”做成一个原子状态变化？
3. 为什么 cond_wait 不能像第 25 课 wait 一样无脑从头 retry？
4. signal 为什么不等于“条件现在一定成立”？
5. 为什么 cond_wait 返回前必须重新拿到 mutex？
6. condvar 为什么不应该被理解成“保存通知次数的计数器”？

## 下一课为什么自然出现

Condvar 的 predicate 保存在共享数据里，Thread 自己通过 mutex + while 判断。

另一类问题更像：

```text
“现在还有 3 个资源许可，谁拿走一个就减一，归还就加一”
```

这正适合 semaphore。

进入 [第 34 课：一次允许几个线程进入](34-semaphore.md)。