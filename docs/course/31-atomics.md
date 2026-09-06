# 第 31 课：原子操作能保护什么

状态：待开始。前置：[第 30 课](30-threads.md) 验收完成。分 A、B 两次，每次 45～60 分钟。

## 这课先把三个概念拆开

很容易把下面三句话混成一句：

```text
“用了原子”
“加了锁”
“关了中断”
```

它们解决的问题不同。

本课目标：

1. 理解一个 atomic RMW 能保证单个操作不可被拆开；
2. 理解多个字段组成的不变量仍需要一个整体临界区；
3. 理解当前**单 hart + kernel 不可抢占**模型下，kernel 自旋锁和 interrupt masking 有特殊边界；
4. 先认识 Acquire/Release，不要求掌握所有弱内存模型。

阅读 OSTEP 第 28～29 章和 Rust `core::sync::atomic::Ordering`。

---

## A 次：一个原子字段不等于一个原子业务操作

## 先复盘 `fetch_add`

第 29 课：

```text
AtomicUsize::fetch_add(1)
```

可以让多个 Thread 对**这个计数器的一次加一**不丢失。

但现在构造一个复合不变量：

```text
account_a + account_b == 100
```

转账 10：

```text
a -= 10
b += 10
```

即使 a 和 b 各自都是 AtomicUsize，如果读者正好在两步之间观察：

```text
a=40
b=50
```

会看到总额 90。

单个字段“没有撕裂”并不意味着：

```text
两个字段作为一件事同时完成
```

## 宿主机先用标准 Mutex 保护复合不变量

实验：

```text
Mutex<AccountPair>
```

所有：

```text
transfer
read_total
```

都通过**同一把 mutex**访问。

验证：

```text
total 始终 100
```

如果写者加锁、读者绕过锁直接读 atomic 字段，仍可能看到中间状态。所以“保护范围”必须覆盖所有需要观察同一不变量的访问者。

---

## Acquire / Release 先只学锁需要的最小直觉

一个最小 spin flag 概念：

```text
unlocked = false
locked   = true
```

获取：

```text
compare_exchange(false → true, Acquire, ...)
```

释放：

```text
store(false, Release)
```

先这样理解：

```text
Release unlock
→ 保证临界区里在它之前的写不会被“发布到锁外以后”

Acquire lock success
→ 后来的持锁者可以按同步关系看到前一持锁者在 Release 前发布的状态
```

不要把它简化成：

```text
Acquire = 读
Release = 写
```

真正规则以后可继续深入。

### Relaxed 什么时候够

如果某个 atomic 只是独立统计：

```text
interrupt_count.fetch_add(1, Relaxed)
```

且这个计数不承担“发布其他内存内容”的同步责任，Relaxed 可能足够。

如果这个 flag 是保护普通共享数据的锁，就需要正确的 Acquire/Release 同步。

### CAS failure ordering

`compare_exchange` 失败路径没有“成功获得锁”，通常不需要 Release；教学实现可用 `Relaxed` failure ordering，并明确成功/失败两条语义不同。

本课不要求背所有 CAS ordering 合法组合，编码时查 Rust 文档。

---

## B 次：单核 kernel 为什么“自旋”特别危险

## 用户态 spinlock 和 kernel spinlock 先分开

### 用户 Thread 自旋

Thread A 持用户自旋锁：

```text
A running, lock held
→ timer preempt A
→ B running, spins
→ 下一个 timer 又能 preempt B
→ A 未来还能重新运行并 unlock
```

所以单核用户 spinlock**可能进展**，只是浪费 B 的 CPU time，并可能造成优先级反转/延迟。

这不表示用户 spinlock 是好选择；只是“holder 仍有机会被 scheduler 恢复”。

### 当前 kernel 的关键路径不同

neonos 当前 S-mode kernel：

```text
不会被 scheduler 在任意 Rust 调用栈中抢占
很多短临界区还会关闭本地 S interrupt
```

如果 kernel 在这种状态下：

```text
while lock is held by another Thread:
    spin
```

而那个 holder 只有“被 scheduler 重新运行”才能 unlock：

```text
当前 kernel 不让出
→ holder 永远得不到 CPU
→ 永久 spin
```

这不是性能差，而是逻辑死锁。

## 单 hart kernel 短临界区的第一版保护

当前阶段最容易证明的内核内部互斥方式：

```text
save local interrupt-enable state
→ disable relevant local S interrupts/preemption entry
→ 做非常短的 metadata 更新
→ restore 原状态
```

例如：

```text
Ready queue
Thread state
wait queue
sync object metadata
```

在只有一个 hart 时，只要：

- 没有另一个 hart 同时执行；
- 当前 kernel 路径不会主动 schedule/sleep；
- interrupt handler 也不能在临界区中重入同一状态；

就不存在“另一个执行者同时修改”。

因此第一版很多 kernel metadata 根本不需要一个会竞争的 spinning lock；**关本地中断/保持 kernel non-preemptible 就是当前模型的一部分。**

## 但“关中断”不是通用锁

未来多 hart：

```text
hart 0 disable interrupt
```

完全不能阻止：

```text
hart 1 同时改同一结构
```

所以：

```text
local interrupt masking
≠ multiprocessor mutual exclusion
```

本课代码和注释要把这个阶段边界写清楚，不把单核技巧包装成通用 SMP spinlock。

## 保存/恢复原 interrupt 状态，而不是一律打开

错误：

```text
disable interrupts
...
enable interrupts
```

如果调用者进入函数前本来就是 disabled，函数返回却擅自打开，会破坏外层临界区。

正确接口更像：

```text
old = irq_save_disable()
...
irq_restore(old)
```

嵌套：

```text
outer old = enabled → disable
inner old = disabled
inner restore(disabled)
outer restore(enabled)
```

不会在 inner 返回时意外打开。

## 临界区里禁止什么

当前内核短 irq-off 区间：

```text
不要 sleep / Blocked
不要 schedule
不要做无界循环
不要等待另一个 Thread 才会改变的条件
不要做慢 UART 大量打印
不要执行可能重新进入同一 metadata 的复杂操作
```

目标是“几个确定的内存状态变更后立刻离开”。

## 中断 handler 和普通 kernel 路径共用状态

如果 timer/UART-poll handler 会访问某个结构，那么普通 S-mode 路径更新它时也必须遵守同一临界区规则。

只保护“用户 Thread 同时访问”，却忘了 interrupt path，也会产生竞态。

## 不用 `volatile` 伪装同步

再次确认：

```text
volatile MMIO
→ 设备访问语义

Atomic/lock/irq discipline
→ 并发同步语义
```

两者不是替代关系。

## 一个确定性 kernel 停滞模型

不要真的在正常 QEMU 路径写一个永远 spin 的 bug。

用状态轨迹模拟：

```text
Thread A acquires user/kernel resource L
A 被切走
Thread B 进入 kernel
B 关 interrupt / 禁止 schedule
B spin wait L
```

问：谁还能运行 A 来 unlock？

答案：没人。

这个推理会直接引出下一课：拿不到用户 mutex 时应该 **Blocked**，而不是占着 CPU 自旋。

## 常见问题

| 现象/误解 | 先检查 |
| --- | --- |
| 每个字段都 Atomic，但总额观察仍错误 | 复合不变量是否有统一临界区 |
| SeqCst 以后就不需要锁 | ordering 不会自动把多个操作变成一个事务 |
| kernel 单核 spin 永不退出 | holder 是否只能靠被 scheduler 恢复 |
| inner 临界区返回后突然进 interrupt | 是否无条件 enable，而没 restore 原状态 |
| “关中断就是 SMP lock” | 其他 hart 根本不受本 hart SIE 影响 |
| volatile 被当作原子/锁 | 两种语义完全不同 |

## 最终验收

### Atomic/临界区

- [ ] 能构造“两个 Atomic 字段但复合 invariant 暂时破坏”的例子。
- [ ] 标准 Mutex 保护全部读/写后 invariant 始终成立。
- [ ] 能解释 Acquire success / Release unlock 的基本发布关系。
- [ ] 知道 Relaxed counter 和 lock flag 的 ordering 责任不同。

### 单核 kernel

- [ ] 能解释 user spin 可能进展但浪费 CPU。
- [ ] 能解释 kernel irq-off/nonpreemptible 状态下 spin 等另一个 Thread 为什么死锁。
- [ ] 内核 metadata 短临界区保存/恢复原 interrupt state。
- [ ] 临界区无 sleep/schedule/慢操作。
- [ ] 明确该方案不是未来 SMP 互斥。

### 理解验收

不看正文回答：

1. atomic field 和 atomic invariant 有什么区别？
2. Acquire/Release 在锁里分别承担什么方向的同步？
3. 为什么 user spinlock 在单核上可能进展，而某些 kernel spin 会永久停住？
4. `irq_restore(old)` 为什么比 `enable_interrupts()` 安全？
5. 关本地中断为什么阻止不了另一个 hart？
6. volatile 为什么不能替代 atomic/lock？

## 下一课为什么自然出现

对用户 Thread 的较长临界区，拿不到锁时一直 spin 会浪费时间片；在 kernel 里不恰当 spin 甚至会死锁。

下一步实现：

```text
拿不到 mutex
→ Blocked
→ holder unlock
→ waiter Ready
```

进入 [第 32 课：拿不到锁，就先让 CPU 做别的事](32-mutex.md)。