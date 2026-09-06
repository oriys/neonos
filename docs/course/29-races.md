# 第 29 课：两个加一，为什么可能只加了一次

状态：待开始。前置：[第五阶段](stage-05.md) 验收完成。预计 45～60 分钟。

## 本课先学“交错”，不急着造锁

两个线程都想执行：

```text
counter += 1
```

直觉会说：

```text
0 + 1 + 1 = 2
```

但这行高级语言操作可能拆成：

```text
load counter
compute old + 1
store new
```

如果两个执行路线交错，最终完全可能只得到 1。

阅读 OSTEP 第 26 章。

## 先分清两个词

### data race

在 Rust 的内存模型里，对同一普通内存位置发生不受同步保护的冲突并发访问（至少一个写）可能构成 data race，而 data race 是 undefined behavior。

所以本课**不**写：

```rust
static mut COUNTER: usize
```

然后让两个 host thread 随便读写，再拿偶然结果当教材。

### logical race / race condition

即使每一次单独读写都使用合法 `AtomicUsize`，多个原子操作组合起来仍可能不满足我们想要的高层不变量。

本课稳定重现的是这一种。

---

## 先手工排一次必错交错

初值：

```text
counter = 0
```

两个线程执行“分开的 load + store”：

```text
A: old_a = load()   → 0
B: old_b = load()   → 0
A: store(old_a + 1) → 1
B: store(old_b + 1) → 1
```

最终：

```text
1
```

这不是因为某次 `AtomicUsize::load/store` 被撕裂；每个原子操作本身都完成了，只是“读-改-写整体”不是一个原子事务。

单核也会出现同类逻辑：

```text
A load
→ timer preemption
B load/store
→ 切回 A
A store
```

所以“只有一个 CPU core”不等于“多条用户执行路线永远不会交错”。

---

## 宿主机确定性实验：不用 sleep 碰运气

计划创建：

```text
experiments/races.rs
```

这个程序使用宿主机 `std`，不要通过仓库默认裸机 target 构建。可以单独：

```sh
rustc --edition=2024 experiments/races.rs -o /tmp/neonos-races
/tmp/neonos-races
```

## 错误版本

共享：

```text
Arc<AtomicUsize>
Barrier(2)
```

两个 worker 都做：

```text
old = counter.load(SeqCst)
barrier.wait()
counter.store(old + 1, SeqCst)
```

Barrier 只包含两个 worker；main thread 不参与这次 barrier，只负责最后 `join`。

因为两次 load 都必须发生在任何 store 之前：

```text
old_a = 0
old_b = 0
```

所以最终稳定为：

```text
1
```

### 一个重要结论

即使使用：

```text
SeqCst
```

这种非常强的原子排序，也不会自动把：

```text
load
+
计算
+
store
```

合并成一个“逻辑原子加一”。

内存顺序和“复合操作是否原子”是不同问题。

---

## 修复版本：真正使用一个 read-modify-write 原子操作

改成：

```text
barrier.wait()
counter.fetch_add(1, SeqCst)
```

两个线程完成后：

```text
counter == 2
```

不要保留错误版“load 以后才 barrier”的结构再混入 `fetch_add`，让修复实验保持简单：两个线程只对齐起跑，然后各做一次真正原子的 RMW。

随后可以把 `fetch_add` 的 Ordering 改成 `Relaxed` 再观察最终计数仍为 2，并讨论：

> 单纯“唯一地加一”不需要靠 SeqCst 排序其他数据；但如果这个原子还承担发布/获取其他共享状态的同步语义，就必须重新设计 ordering。

本课不要求掌握完整弱内存模型，第 31 课再讲 Acquire/Release。

---

## 为什么 `volatile` 不能修这个问题

`volatile` 适合表达：

```text
这个内存访问本身有外部设备副作用，编译器不能把它当普通内存随意消掉
```

它不提供：

```text
线程间原子 RMW
互斥
happens-before
```

所以第 02 课 UART 的 `write_volatile` 不能拿来替代原子操作或锁。

## 再做一个单线程“调度交错模拟”

为了把现象和 neonos 单核联系起来，用一个普通宿主机状态机显式执行：

```text
A_Load
B_Load
A_Store
B_Store
```

然后换顺序：

```text
A_Load
A_Store
B_Load
B_Store
```

前者结果 1，后者结果 2。

这样学生能看到：**决定结果的是允许的交错，而不是“线程库随机抽风”。**

## 常见问题

| 现象 | 先检查 |
| --- | --- |
| 错误版有时得到 2 | 两个 load 之后是否真的有 Barrier(2) |
| 程序一直等 | Barrier 参与者数量是否与实际 wait 的 worker 一致 |
| 改成 Atomic 仍丢更新 | 是否仍使用分开的 load/store，而不是一个 RMW |
| 以为 SeqCst 自动等于锁 | 是否混淆 memory ordering 与复合不变量 |
| 想换成 volatile | volatile 根本不是线程同步原语 |

## 验收

### 实验

- [ ] 分离 load/store 版本重复运行稳定得到 1。
- [ ] `fetch_add` 版本稳定得到 2。
- [ ] 单线程交错模型能得到 1 和 2 两种可解释结果。
- [ ] 全部实验没有使用 Rust data race / `static mut` UB。

### 理解

不看正文回答：

1. data race 和 logical race 有什么区别？
2. 为什么两个 SeqCst load/store 仍可能丢失更新？
3. `fetch_add` 比 `load+store` 多保证了什么？
4. 为什么单核也会发生逻辑竞争？
5. volatile 为什么不能修共享计数竞争？
6. `Relaxed fetch_add` 能保证计数唯一增加，但为什么不等于能发布任意其他共享数据？

## 下一课为什么自然出现

现在已经知道“多条执行路线共享内存”会产生竞争，但 neonos 目前的一个 Process 只有一个调度现场。

下一步先真正实现 thread：

```text
同一个 AddressSpace
+
多个独立 user context/stack
+
同一个 scheduler
```

然后再回来用锁保护共享状态。

进入 [第 30 课：同一个进程里，运行多条执行路线](30-threads.md)。把两个确定性实验和自己的交错图写进 [进度记录](progress.md)。