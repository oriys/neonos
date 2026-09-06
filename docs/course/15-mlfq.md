# 第 15 课：让调度器根据运行表现调整优先级

状态：待开始。前置：[第 14 课](14-preemption.md) 验收完成。分 A、B、C 三次，每次 45～60 分钟。

## 这课真正要学什么

RR 只有一个 Ready queue，所有任务轮流拿相同 quantum。

MLFQ 试图利用“过去的运行行为”近似判断：

```text
经常很快让出/等待的任务
可能更像交互任务

持续消耗 CPU 的任务
可能更像计算任务
```

但如果规则设计不好，程序可以故意频繁 `yield` 或 syscall 来永远待在高优先级。

所以本课的核心不是背“有三条队列”，而是：

> **明确写出优先级、时间片、累计配额、周期提升和计账规则，再用确定性实验证明任务不能靠切碎执行逃避计费。**

阅读 OSTEP 第 8 章，并可参考作者的 MLFQ 模拟实验说明。本课程的具体数字是 neonos 教学配置，不是 MLFQ 唯一标准。

---

## A 次：先在模拟器里把规则写死

### 课程配置

用基础时间单位 `T`：

| 队列 | 优先级 | 单次 slice | 本级累计 allotment |
| --- | --- | --- | --- |
| Q0 | 最高 | 1T | 2T |
| Q1 | 中 | 2T | 4T |
| Q2 | 最低 | 4T | 不再降级，持续 RR |

### 六条规则

1. 新任务进入 Q0。
2. 总是选择最高的非空 Ready queue；同级 RR。
3. 一个 slice 用完但本级 allotment 未用完：回本级队尾，下一次获得新的本级 slice。
4. 本级累计 allotment 用完：降一级，进入新级队尾，并初始化新级的 slice/allotment。
5. 主动 `yield`：回原级队尾，但**不重置剩余 slice，也不清零累计 allotment**。
6. 每隔 20T 做一次全局 boost：所有活跃任务回 Q0，并重置 Q0 的 slice/allotment；模拟中的 Blocked 任务只改变级别/预算，不因为 boost 凭空变 Ready。

### 为什么要同时有 slice 和 allotment

假设 Q0：

```text
slice = 1T
allotment = 2T
```

任务每运行 `0.5T` 就 yield：

```text
第一次：累计用了 0.5T，slice 剩 0.5T，allotment 剩 1.5T
第二次：再用 0.5T，slice 用完；allotment 还剩 1T
下一次得到新的 1T slice
继续累计
```

它不能因为“每次都没跑满一整片”就无限保留 Q0。

这就是本课必须手算的一条反作弊轨迹。

### 固定模拟事件顺序

计划创建：

```text
experiments/mlfq.py
```

每个事件边界统一：

```text
1. 结算刚才实际执行了多少 CPU 时间
2. 处理完成/退出/等待等状态变化
3. 处理同刻到达或唤醒
4. 根据 slice/allotment 决定原任务回队或降级
5. 处理到期的全局 boost
6. 从最高非空队列选择下一个
```

如果同一时刻既发生“刚好用完 allotment”又发生 boost，本课程按上面顺序：先做本次用量导致的状态变化，再 boost。模拟和内核都要保持一致。

### 四类固定负载

至少比较：

- 长纯计算任务；
- 晚到短任务；
- 模拟 I/O 等待任务；
- 高频 yield 任务。

模拟器的等待只是策略实验，不代表 neonos 内核此时已经有真实 I/O Blocked 队列。

### A 次验收

- [ ] 能手算高频 yield 任务最终仍会耗尽 Q0 allotment。
- [ ] Q0/Q1/Q2 的 slice 与 allotment 含义不混淆。
- [ ] boost 的同刻顺序明确。
- [ ] 模拟轨迹是确定性的，同一输入重复运行相同。

---

## B 次：把“时间计账”接进内核，而不是数中断次数

### 先定义每个运行线程/任务至少三个量

```text
last_user_enter
slice_remaining
allotment_remaining
```

以及：

```text
level
next_global_boost_deadline
```

### 每次真正进入用户前

```text
last_user_enter = read_time()
```

这里不是重置预算，只记录“从这一刻开始又在用户态真正执行”。

### 每次从用户 trap 回内核时

第一件调度计账工作：

```text
now = read_time()
delta = now - last_user_enter
slice_remaining     -= delta
allotment_remaining -= delta
```

使用检查/饱和逻辑避免延迟导致整数下溢；如果实际处理点已经超过预算边界，记录 overshoot，再把剩余量视为 0。

这样：

```text
timer trap
syscall
yield
exit
user fault
```

都会结算这一次真实用户执行区间。

### 为什么不能“每次 timer interrupt 算 1 个时间片”

如果只在 timer 时计费：

```text
任务运行 0.9T
→ yield
→ 没 timer
→ 你记 0
```

重复后它就可以永远不降级。

同理，频繁普通 syscall 也不能重置 budget。

### syscall 返回同一个任务时怎么做

例：任务剩：

```text
slice_remaining=0.4T
allotment_remaining=1.4T
```

它执行 0.1T 后 `putchar`：

```text
结算后：
slice_remaining=0.3T
allotment_remaining=1.3T
```

内核处理 `putchar` 的时间不算用户执行时间。返回用户前：

```text
last_user_enter = new_now
```

但**预算仍然是 0.3T / 1.3T**，不是重新变回完整 Q0 配额。

如果结算时 slice/allotment 已经耗尽，就先在安全调度边界执行回队/降级，不先返回用户再白送时间。

### 下一次 timer deadline 取最早约束

返回用户前计算：

```text
min(
  slice_remaining,
  allotment_remaining,
  time_until_next_boost
)
```

并预约对应绝对 deadline。

这样 timer 不只是“一个固定 T 的节拍器”，而是在下一个调度规则边界把控制权交回来。

### slice 用完和 allotment 用完的处理

- `slice_remaining == 0` 且 allotment 仍有：回本级队尾，下一次 dispatch 初始化新的本级 slice。
- `allotment_remaining == 0`：降级；新级别初始化该级 slice/allotment。
- Q2 allotment 不导致进一步降级，只按 Q2 slice RR。

主动 yield 不执行“新 slice”初始化；它保留当前剩余 slice，等以后重新被选中继续使用。

### 全局 boost

如果 `now >= next_boost_deadline`：

1. 在 scheduler 安全点执行一次有效 boost；
2. 活跃任务重置到 Q0 预算；
3. Ready queue 按稳定规则重建，避免重复 task；
4. 把 `next_boost_deadline` 向前推进到**严格晚于 now** 的下一个周期。

如果内核一次延迟跨过多个 20T，不需要机械连续做多个等价 reset，但 deadline 必须推进到未来，避免马上反复触发“过期 boost”。

### B 次验收

- [ ] 高频 yield 仍会累计消耗 allotment 并降级。
- [ ] 普通 syscall 不重置 slice/allotment。
- [ ] 内核处理时间没有被当作用户 delta 重复计费。
- [ ] timer deadline 取最早规则边界。
- [ ] boost 后队列无重复，deadline 被推进到未来。

---

## C 次：用同一负载公平比较 RR 和 MLFQ

### 比较之前先固定所有实验条件

必须固定：

```text
任务代码
service/workload
arrival/激活时刻
初始顺序
时间单位 T
日志位置
重复次数
```

RR 基准 quantum=T；MLFQ 用上面的配置。

不能：

```text
RR 用长任务
MLFQ 用短任务
```

然后得出“MLFQ 更快”。

### 模拟器指标

对相同输入记录：

- 平均首次响应；
- 平均周转；
- 最长 Ready 等待间隔；
- 上下文切换次数；
- MLFQ 规则校验结果。

模拟器可以包含晚到和 I/O waiting；内核当前没有真实 I/O Blocked，所以内核对比只使用它实际支持的任务模式。

### 内核晚到任务不要假装成真实 process create

如果为了实验短任务晚到，可由测试管理逻辑在指定时间激活一个**预先准备好的槽位**，并明确日志：

```text
[test activation]
```

不要把这描述成已经实现了用户 `fork/create`。

### 把串口打印放到测量区间外

串口输出会严重影响 QEMU 时间。

测量期间只写固定容量事件记录；任务完成后再汇总。

多次 QEMU 运行的绝对计数可能有波动，所以先验证：

```text
规则是否正确
任务是否完成
事件顺序是否符合设计
```

再解释数值，不把一次偶然测量写成普遍性能结论。

### 记录表

| 负载/策略 | 配置 | 平均响应 | 平均周转 | 最长 Ready 等待 | 切换数 | 规则检查 |
| --- | --- | --- | --- | --- | --- | --- |
| same / RR | q=T | 实测 | 实测 | 实测 | 实测 | pass/fail |
| same / MLFQ | Q0/Q1/Q2 + boost | 实测 | 实测 | 实测 | 实测 | pass/fail |

结论允许是：

```text
这个负载下 RR 某指标更好
```

不要预设“MLFQ 一定获胜”。

## 常见问题

| 现象 | 先检查 |
| --- | --- |
| 高频 yield 永远 Q0 | 是否只按 timer 次数计费，或 yield 时重置预算 |
| putchar 后得到完整新 slice | syscall 返回是否错误初始化 budget |
| 预算突然变成超大整数 | delta 扣减是否发生无符号下溢 |
| boost 后任务重复 | 是否清空/稳定重建 Ready queue，Running 处理是否重复 |
| boost 连续疯狂发生 | next boost deadline 是否还停在过去 |
| RR/MLFQ 数值差得离谱 | 输入、打印量、时间单位是否真的相同 |

## 第三阶段最终验收

### 运行/规则

- [ ] 调度模拟公式和结果可复现。
- [ ] `yield` 可以保存/恢复多个用户任务。
- [ ] timer 能独立连续发生且不破坏用户现场。
- [ ] 两个不 yield 的任务能被 timer RR 抢占。
- [ ] 普通 syscall 不刷新完整 RR/MLFQ 时间预算。
- [ ] MLFQ 高频 yield 仍会降级。
- [ ] boost、降级、最低级 RR 都有确定轨迹。
- [ ] RR/MLFQ 同输入对比有真实记录。

### 理解

不看正文回答：

1. slice 和 allotment 为什么要分开？
2. `last_user_enter` 解决了什么计账问题？
3. 为什么 syscall/yield 也要先结算用户 delta？
4. 为什么内核时间不应该再次算进本课用户 CPU 用量？
5. 为什么下一 deadline 要取多个约束中的最早者？
6. periodic boost 主要防什么问题？
7. 为什么实验不能先假定 MLFQ 一定比 RR 好？

完成 [阶段总验收](stage-03.md) 并记录真实结果。下一阶段从 [第 16 课：同一个地址，能住不同的数据吗](16-addresses.md) 开始学习内存虚拟化。