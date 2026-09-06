# 第 35 课：线程都活着，为什么谁也不前进

状态：待开始。前置：[第 34 课](34-semaphore.md) 验收完成。分 A、B 两次，每次 45～60 分钟。

## 本课先学会“证明死锁”，再谈修复

一个程序超时并不能自动说明：

```text
发生了 deadlock
```

它也可能是：

- 死循环；
- 外部输入永远没来；
- starvation；
- livelock；
- bug 让某个 wakeup 丢了。

本课要求拿出**等待关系证据**。

阅读 OSTEP 第 32～33 章。

---

## A 次：确定性构造一个两锁死锁

## 先认识 Coffman 四个必要条件

经典资源死锁通常同时具备：

```text
1. Mutual exclusion   资源不能被多人同时拥有
2. Hold and wait      持有一个资源时又等待另一个
3. No preemption      资源不会被系统强行安全夺走
4. Circular wait      等待关系形成环
```

本课用 mutex 构造全部四个。

### 目标等待关系

```text
Thread A owns L1, waits L2
Thread B owns L2, waits L1
```

画成 wait-for graph：

```text
A → L2 → B → L1 → A
```

其中：

```text
Thread → Lock = waiting for
Lock → Thread = owned by
```

这个环才是本实验的核心证据。

## 不靠 sleep 碰运气

错误实验如果只是：

```text
A lock L1
sleep(random)
B lock L2
...
```

可能有时死锁、有时直接跑完，很难学习。

使用**测试专用 gate/latch**安排确定顺序：

```text
A 获得 L1 → 报告 A_READY
B 获得 L2 → 报告 B_READY
测试 gate 等两者都 ready
→ 同时允许继续
A 请求 L2
B 请求 L1
```

测试 gate 不是 mutex 实现的一部分，也不能依赖 L1/L2 的错误获取顺序才能自身完成；它只是确定性实验控制器。

## 观察器要输出真正的等待图

在独立 QEMU 测试模式中，有限时间后由测试管理/诊断路径打印：

```text
Thread A: Blocked(Mutex L2)
Thread B: Blocked(Mutex L1)
L1.owner = A
L2.owner = B
Ready queue = ...
```

然后生成/人工画出：

```text
A → L2 → B → L1 → A
```

超时本身只触发“采样诊断”；**检测到环**才是“这个负例符合预期死锁”的通过证据。

测试完成后由 harness 结束该 QEMU 会话，不把 deadlocked kernel 留给后续正例。

---

## 用统一锁顺序修复

为这个锁集合定义：

```text
L1 < L2
```

所有路径都必须：

```text
先 L1
后 L2
```

A/B 都遵守后，不能再形成：

```text
一个持 L1 等 L2
另一个持 L2 等 L1
```

因此对**这个受统一顺序约束的锁集合**消除了 circular wait。

不要写成：

```text
“有全局锁顺序，所以系统再也不会有任何死锁”
```

因为：

- 还有 condvar/semaphore/I/O 等其他等待资源；
- 动态资源图可能不受这套 order 覆盖；
- 错误代码可能绕过规定。

### 修复后测试 gate 也必须改

原负例 gate 要求：

```text
A 先拿 L1
B 先拿 L2
```

修复后如果仍强制 B“先拿 L2”，测试工具本身就在要求违反新锁顺序。

因此正例 gate 只对齐“两个 Thread 都开始”，不再要求它们分别持不同第一把锁。

验证：

```text
两 Thread 都完成
共享结果正确
wait-for graph 无 cycle
```

---

## 区分三种“没前进”

### Deadlock

一组参与者互相等待，形成无法自行打破的等待环/资源依赖。

### Starvation

某个 Thread 长期拿不到资源，但系统其他 Thread 仍持续完成工作。

例如 priority policy 不公平导致低优先级一直被跳过。

### Livelock

Thread 不断运行、不断改变状态/重试，却因为彼此“太积极让步”始终没有实际工作完成。

所以：

```text
CPU 很忙
```

不代表不是并发停滞；livelock 反而可能很忙。

### A 次验收

- [ ] 错误版稳定得到 L1/L2 wait cycle。
- [ ] 负例通过条件包含“检测到等待环”，不是只看 timeout。
- [ ] 统一 lock order 后两个 Thread 都完成。
- [ ] 正例测试 gate 不再依赖旧错误顺序。
- [ ] 能区分 deadlock/starvation/livelock。

---

## B 次：把“等待式 Thread”改写成事件状态机做对照

OSTEP 后面还讨论 event-based concurrency。

本课不是要把 neonos scheduler 改成 event loop，而是做一个宿主机/教学模型对比：

```text
Thread/blocking style
vs
explicit event/state-machine style
```

## 选同一个工作流

例如：

```text
WaitInput
→ HaveInput
→ NeedOutputSpace
→ Done
```

### Thread 版本

逻辑上写成：

```text
read/block
process
write/block
```

等待由 scheduler/condvar/pipe state 隐藏在调用背后。

### Event 版本

把状态显式保存：

```text
enum State {
  WaitingInput,
  ReadyToProcess,
  WaitingOutput,
  Done,
}
```

事件循环每次：

```text
取一个 ready event
→ 做有限工作
→ 更新 state
→ 返回 loop
```

不要在 callback 中：

```text
无限循环
阻塞 read
等待另一个 event 才能完成的 mutex
```

否则一个长 callback 就会把整个单线程 event loop 卡住。

## 用相同输入比较

Thread 版和 event 版都处理同一组固定消息，验证：

```text
输出 bytes 完全一致
最终每个 state 都 Done
```

再比较可观察差异：

```text
Thread 模型：每个执行路线有独立 stack，阻塞点写法直观
Event 模型：显式 state，没有每个任务的阻塞 stack，但控制流被拆成状态转换
```

### event-driven 不等于“不需要同步”

单线程 event loop 中同一时刻只有一个 callback 执行，确实减少某些共享内存并发。

但如果未来：

- 多个 event loop thread；
- DMA/interrupt 与 loop 共享状态；
- callback 把工作交给 worker thread；

仍然需要同步。

也不能因为用了 event loop 就自动获得多核并行。

---

## `tests/concurrency.sh` 应怎样判定

把三类场景分开：

### expected race/logical failure experiment

检查确定的错误结果/交错证据。

### expected deadlock negative

检查：

```text
wait-for cycle 存在
```

然后 harness 主动结束会话。

### fixed positive

检查：

```text
最终 completion marker
所有 Thread join/reap
mutex/condvar/semaphore object count 回基线
Ready/Blocked waiters 清空
共享数据 invariant 正确
```

不要把：

```text
“跑了 5 秒没结束”
```

当成 expected deadlock 的唯一成功条件。

## 第六阶段最终验收

### 并发现象

- [ ] 第 29 课逻辑 race 不依赖 Rust UB。
- [ ] Thread shared AddressSpace / independent context 正确。
- [ ] atomic/compound invariant/irq masking 边界能解释。

### 同步原语

- [ ] mutex direct handoff/pending completion 不重复 acquire。
- [ ] condvar wait 原子 release+sleep，返回前重新持 mutex。
- [ ] semaphore permit direct grant 不重复计数。
- [ ] destroy 拒绝 waiters/pending refs。

### 停滞诊断

- [ ] deadlock 有 wait-for cycle 证据。
- [ ] 统一 lock order 修复后完成。
- [ ] 能区分 deadlock/starvation/livelock。
- [ ] event-state-machine 对照不把长阻塞 callback 偷偷塞回 event loop。

### 回归

- [ ] Process fault/exit 会清理所有 Thread/waiter/sync object。
- [ ] 原 shell/pipe/fork/wait/memory/scheduling 回归通过。
- [ ] 多轮并发测试后 frame/Thread/sync/fd/pipe 资源回基线。

## 理解验收

不看正文回答：

1. Coffman 四个条件是什么？
2. 为什么 timeout 不能单独证明 deadlock？
3. wait-for graph 中 Thread→Lock 和 Lock→Thread 各代表什么？
4. 统一 lock order 消除的是哪个条件？为什么只对受这套顺序覆盖的资源成立？
5. starvation 和 deadlock 的系统级进展有什么区别？
6. livelock 为什么可能 CPU 很忙却没工作完成？
7. event-driven 为什么不自动等于“完全不需要同步”？

完成 [第六阶段总验收](stage-06.md) 并更新 [进度记录](progress.md)。下一步从 [第 36 课：先准备一块实验磁盘](36-disk.md) 进入持久化。