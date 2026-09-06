# 第 34 课：一次允许几个线程进入

状态：待开始。前置：[第 33 课](33-condvar.md) 验收完成。分 A、B 两次，每次 45～60 分钟。

## semaphore 和 mutex 最容易混在哪里

两者都可能“阻塞 Thread”，但核心状态不同：

```text
mutex
→ 记录唯一 owner
→ unlock 只能由 owner 做

semaphore
→ 记录可用 permit 数 count
→ sem_post 不要求是同一个 Thread
```

所以：

> **binary semaphore（max=1）也不自动等于带 owner 语义的 mutex。**

阅读 OSTEP 第 31 章。

---

## A 次：先把 permit 守恒做正确

## 对象状态

semaphore 至少保存：

```text
handle/generation
count      # 当前“空闲可领取”的 permit
max
FIFO waiters
pending_grants / pending refs
```

创建：

```text
sem_create(initial,max)
```

规则：

```text
max > 0
0 <= initial <= max
max <= 课程固定上限
```

非法：

```text
-2
```

对象表满：

```text
-4
```

## `sem_wait`

### 有 permit

在短 kernel metadata 临界区：

```text
count > 0
→ count -= 1
→ return 0
```

Thread 此后“持有一个业务许可”，但 semaphore 不记录 owner是谁。

### 没 permit

```text
count == 0
→ enqueue current TID once
→ pending = SemWaiting(handle)
→ Thread = Blocked(Semaphore(handle))
→ 回 scheduler
```

一个重要不变量：

```text
waiters 非空时，count 应保持 0
```

如果实现出现：

```text
count > 0
同时还有 FIFO waiter
```

说明 permit 分配/公平规则已经不一致。

## `sem_post`

### 有 waiter：直接交付，不增加 count

```text
pop FIFO waiter W
→ W.pending = SemGranted(handle)
→ W Blocked→Ready
→ count 保持 0
```

这一枚 permit 已经直接给 W 了。

错误做法：

```text
count += 1
同时又 wake W 并说它拿到 permit
```

这样同一 permit 被记了两次。

### 没 waiter：才增加 count

```text
if count < max:
    count += 1
    return 0
else:
    return -2
```

超过 max 不允许 silently saturate；错误不改变 count。

## 被 direct grant 的 waiter 怎样完成原 ecall

和 mutex 一样，W 的原 `sem_wait` 还停在原 ecall。

重新执行时 dispatcher 先看：

```text
pending == SemGranted(same handle)
```

成立：

```text
consume pending
→ a0=0
→ sepc += 4
→ return user
```

**不要再次 `count -= 1`。**

否则 direct grant 的 permit 会被重复消费。

### pending grant 也是对象引用

在 `SemGranted` 尚未被 Thread 消费前：

```text
sync_destroy(handle)
```

必须：

```text
-9
```

不能把对象槽复用，然后 Ready Thread 以后拿旧 handle 完成一笔已经不存在的 permit。

如果整个 Process 在 grant 消费前终止，本阶段会一起销毁所有 Thread 和 sync objects，所以没有“继续运行但 permit 丢失”的语义；cleanup 必须把 pending state 一并取消。

---

## permit 守恒怎样验证

仅看 `count` 不够，因为一些 permit 可能已经被 Thread 取走正在使用。

对一个测试资源池定义：

```text
available = sem.count
in_use    = 已成功 wait 但尚未 post 的业务 permit 数
```

在没有 pending handoff 的稳定检查点：

```text
available + in_use == max
```

若刚发生 direct handoff、W 还没执行到 user：

```text
available + in_use + granted_in_transit == max
```

这比只断言：

```text
count <= max
```

更能发现 permit 丢失/重复。

## 三 Thread / max=2 实验

三个 worker：

```text
sem_wait
→ 进入资源区
→ active += 1
→ 做有限工作
→ active -= 1
→ sem_post
```

`active` 本身也是共享状态，必须用 mutex/atomic 正确保护；否则你可能只是“统计器自己 race”，误以为 semaphore 失败。

记录：

```text
max_observed_active <= 2
```

且每个成功 wait 恰好对应一次 post。

semaphore API 不会自动知道业务代码忘了 post；这是调用者责任。

### A 次验收

- [ ] `wait` 有 permit 才减 count；无 permit Blocked。
- [ ] `post` 有 waiter 时 direct grant，不同时增加 count。
- [ ] waiter 通过 `SemGranted` 完成，不二次减 count。
- [ ] count/max/pending/in_use 守恒可核对。
- [ ] `sem_post` 不检查“是不是原 wait 的那个 Thread”。

---

## B 次：用 semaphore 重写 bounded queue

第 33 课：

```text
mutex + not_empty condvar + not_full condvar
```

本课改成经典三对象：

```text
empty = Semaphore(initial=N, max=N)
full  = Semaphore(initial=0, max=N)
mutex = 第 32 课 mutex
```

### producer

```text
sem_wait(empty)
mutex_lock(m)
push(item)
mutex_unlock(m)
sem_post(full)
```

### consumer

```text
sem_wait(full)
mutex_lock(m)
item = pop()
mutex_unlock(m)
sem_post(empty)
```

## 为什么不能拿着 mutex 再等 empty/full

错误 producer：

```text
lock(m)
sem_wait(empty)  # 可能 Blocked
```

如果 queue full：

```text
producer 持 mutex Blocked
consumer 想 lock(m) 才能 pop
→ consumer 永远拿不到 mutex
→ deadlock
```

所以 permit wait 必须发生在进入 queue mutex 之前。

## 中途时刻不要错误断言 `empty + full == N`

例如 producer：

```text
sem_wait(empty) 已成功
但还没 push / post(full)
```

这时一枚 permit 正在 producer 手里：

```text
empty.count + full.count < N
```

完全正常。

完整守恒更像：

```text
available empty permits
+ queue items represented by full permits
+ producer/consumer 正在持有的 in-flight permit
= capacity（按阶段定义细分）
```

所以只在明确的 quiescent/stable checkpoint 用 `empty+full=N`，运行中用“permit + in-flight + queue state”解释。

## 和 Condvar 版本做概念对照

### Condvar

```text
predicate 存在共享数据：len==0 / len==N
通知本身不存次数
user while 反复检查 predicate
```

### Semaphore

```text
permit count 本身就是同步状态
成功 wait 真正消费一枚 permit
post 真正产生/归还一枚 permit
```

两者都能做 bounded queue，但“保存条件的地方”不同。

## 销毁规则

`sync_destroy(sem)` 只有：

```text
waiters empty
pending grants empty
没有其他对象/Thread 持有内核引用
```

才允许。

注意：semaphore 不知道某个用户 Thread 是否在业务层“拿了 permit 但忘记 post”。如果业务仍在使用资源却把对象 destroy，是用户协议错误；课程测试要求调用者在销毁前完成所有 worker/join 和 permit 回收。

## 常见问题

| 现象 | 先检查 |
| --- | --- |
| 一次 post 让两个 waiter 都通过 | 是否既 count++ 又 direct grant |
| waiter 醒后 count 变成 -1/下溢 | `SemGranted` 完成时是否再次 decrement |
| queue full 后全部卡住 | 是否持 queue mutex Blocked 在 empty/full |
| active 偶尔 > max | semaphore 逻辑或 active 统计自身同步是否错误 |
| destroy 后 Ready waiter 用旧 handle | pending grant 是否被当成对象引用 |
| `empty+full` 暂时小于 N 就判失败 | 是否忘了 in-flight permits |

## 最终验收

### semaphore

- [ ] initial/max 边界和 overflow 正确。
- [ ] FIFO wait/direct grant 无重复 permit。
- [ ] `waiters>0 ⇒ count==0` 不变量成立。
- [ ] stable/pending 时 permit 守恒能解释。
- [ ] post 可由不同 Thread 调用，不使用 mutex owner 规则。
- [ ] destroy 拒绝 waiters/pending grants。

### bounded queue

- [ ] max=1、max=N 资源实验都不超过容量。
- [ ] semaphore queue 与 condvar queue 得到同一数据集合结果。
- [ ] 不持 mutex 等 empty/full。
- [ ] 运行中能正确解释 in-flight permit，而不是机械 `empty+full=N`。

### 理解验收

不看正文回答：

1. semaphore count 表示“空闲许可”还是“当前使用者数量”？
2. 为什么有 waiter 时 post 不再 count++？
3. `SemGranted` waiter 为什么不能再次 decrement？
4. binary semaphore 为什么仍不等于 mutex？
5. bounded queue 为什么先 wait(empty/full) 再拿 queue mutex？
6. 为什么运行中 `empty.count + full.count` 可能小于 N？

## 下一课为什么自然出现

现在已经有多种阻塞同步：

```text
mutex
condvar
semaphore
```

它们组合错误以后，所有 Thread 可能都“状态正常、没有 crash”，却再也不前进。

下一课专门学习这种停滞：[第 35 课：线程都活着，为什么谁也不前进](35-deadlocks.md)。