# 第 14 课：程序不让出，内核也能切换

状态：待开始。前置：[第 13 课](13-timer.md) 验收完成。分 A、B 两次，每次 45～60 分钟。

## 这课只把两种已有能力接起来

已经分别会：

```text
第 12 课：保存任务现场 → Ready queue → 切换任务
第 13 课：timer 到期 → 用户自动 trap 回内核
```

现在组合成抢占式 RR：

```text
用户不调用 yield
  ↓
timer 到期
  ↓
保存当前任务
  ↓
放回 Ready 队尾
  ↓
选下一个任务
```

复习 OSTEP 第 6～7 章，始终区分：

```text
mechanism：怎样保存/恢复现场
policy：下一项选谁
```

---

## A 次：接入最小 RR 时间片

### 先定义本课 quantum 语义

第一版把：

```text
quantum = 从一次真正 dispatch 到预定 timer deadline 的墙钟 time 计数区间
```

它不是精确的“用户指令 CPU 时间”。如果任务在时间片中间做普通 syscall，内核处理时间也可能让绝对 deadline 更接近甚至过期。

这是本课的刻意简化；第 15 课会改成更细的用户执行用量结算。

最重要的规则是：

> **普通 syscall 返回同一个任务时，绝不能重新赠送一个完整新 quantum。**

否则程序可以不断 syscall 来逃避 timer。

### 什么情况下获得一个新 quantum

只有 scheduler 真正完成一次新的 dispatch 时：

```text
选择一个 Ready task
→ task = Running
→ deadline = now + quantum
→ set_timer(deadline)
→ sret
```

如果同一个任务因为普通 `putchar` 等 syscall trap 进内核又立即返回：

```text
保持原 deadline
```

不要 `now + quantum` 重置。

如果原 deadline 已经在内核处理期间过期，可以让 pending timer 在返回用户后立即到达；也可以在安全返回点检测过期并直接走调度动作。无论哪种，**不能凭空增加预算**，并把选择写清楚。

### timer trap 的状态转换

用户 timer interrupt：

```text
Running
  ↓ 保存 persistent context
Ready
  ↓ enqueue tail
回中央 scheduler loop
```

timer 是 interrupt，不是 `ecall`：

```text
sepc 保持原语义，不 +4
```

如果最后 scheduler 又选择到同一个任务，也必须通过正常的保存/恢复路径完成，而不是因为“还是自己”就跳过状态处理。

### 双任务实验必须没有主动调度点

准备 A、B 两个长整数计算任务：

- 内层循环不调用 `yield`；
- 内层循环不调用 `putchar`；
- 各自在用户寄存器/栈中维护私有数据；
- 完成后才通过 syscall 输出校验结果并 `exit`。

调整工作量或 quantum，确保每个任务在完成前至少经历若干 timer trap。

真正证明抢占的证据不是“串口上 A/B 字符交错”，而是日志中出现：

```text
dispatch A
timer A → Ready
dispatch B
timer B → Ready
...
```

同时 A/B 的最终计算结果仍正确。

### A 次验收

- [ ] 两个完全不 yield 的用户任务都能完成。
- [ ] 至少观察到 timer 导致的 A→B 和 B→A。
- [ ] timer 保存的 `sepc` 不被 syscall 规则误改。
- [ ] 普通 syscall 不重置当前完整 quantum。
- [ ] 最终计算校验正确。

---

## B 次：处理抢占边界，而不制造内核重入

### 内核关键路径保持不可抢占

当前课程仍采用：

```text
U-mode 可以被 S timer 打断
S-mode scheduler/trap/console 路径不主动允许同级 timer 嵌套
```

用户 trap 进入 S-mode 后，更新：

- current task；
- Ready queue；
- persistent context；
- `stvec/sscratch`；
- 资源状态；

期间保持 S-mode 同级中断关闭。

不要为了追求“更实时”在尚不支持重入的打印、队列更新或现场切换中间重新开 timer interrupt。

### deadline 在内核期间到期怎么办

由于本课使用绝对 deadline，可能发生：

```text
用户在 quantum 内先 syscall
→ 进入内核
→ 内核处理较慢
→ deadline 在 S-mode 期间到期
```

本阶段允许 timer delivery 被推迟到安全边界，不能因此声称精确硬实时。

关键要求只有两个：

```text
不嵌套破坏内核状态
不因为 syscall 重新送完整 quantum
```

### exit / fault 与 timer 共用同一个 scheduler 汇合点

不同用户 trap 最终形成明确 outcome：

```text
syscall continue → ResumeUser
 yield            → Ready + Schedule
 timer            → Ready + Schedule
 exit             → Exited + Schedule
 user fault       → Faulted + Schedule
```

不要为 timer 再复制一个平行 scheduler。

### 空队列的本阶段含义

当前没有 I/O Blocked 任务：

```text
Ready queue empty
AND
没有 Running
```

意味着所有测试任务都已 `Exited/Faulted`。

此时：

1. 清除 STIE；
2. `set_timer(u64::MAX)`；
3. 输出阶段完成摘要；
4. 进入稳定等待。

第 25、28 课加入 Blocked/外部输入后，空 Ready queue 的语义会改变。

### 边界测试

至少跑：

- 单任务，无 yield；
- 两任务长计算；
- 一个任务提前 exit；
- 一个任务受控 user fault；
- 普通 syscall 发生在时间片中途；
- 全部任务结束；
- 内核故障独立会话。

### 新增阶段脚本

计划创建：

```text
tests/scheduling.sh
```

它至少检查：

```text
真正存在 timer switch reason
每个任务完成/故障次数正确
计算 checksum 正确
最终完成标记出现
无意外 panic/kernel fault/failure marker
超时后打印完整状态摘要
```

不能只 grep “timer” 一词就成功。

## 常见问题

| 现象 | 先检查 |
| --- | --- |
| timer 一直发生但总是 A | A 是否真的入队尾、current/queue 是否一致 |
| syscall 很频繁的任务几乎不被抢占 | syscall 返回是否重置了 deadline/quantum |
| 程序越切换栈越深 | 是否又递归进入 scheduler，persistent context 是否正确 |
| Faulted/Exited 任务再次出现 | terminal state 是否被错误入队 |
| 打印中偶发重入/损坏 | S-mode 中是否意外开放 timer 嵌套 |
| 全部结束后还不停进 timer | STIE/deadline 是否真正关闭 |

## 最终验收

### 运行验收

- [ ] 两个无 yield 长任务被 timer 抢占并完成。
- [ ] 普通 syscall 不获取一个全新完整时间片。
- [ ] exit/fault 只结束对应任务，其他 Ready 任务继续。
- [ ] Ready queue 无重复，最多一个 Running，终态不再运行。
- [ ] 全部结束后 timer 停止。
- [ ] 原用户/启动测试和新增 scheduling test 都通过。

### 理解验收

不看正文回答：

1. timer interrupt 提供的是 mechanism 还是 policy？
2. RR 的 Ready queue 选择规则是什么？
3. 为什么普通 syscall 不能重置 quantum？
4. 为什么 timer interrupt 不改变 `sepc`？
5. 为什么当前内核关键路径选择不被 timer 嵌套抢占？
6. 为什么这个 RR 还不能称为精确 CPU-time 计费？
7. 为什么此时 Ready queue 空可以表示“全部结束”，以后却不一定？

## 下一课为什么自然出现

RR 让每个 Ready 任务轮流获得 CPU，但它对“短而交互”和“长计算”一视同仁。

下一课开始问策略问题：

> **能不能根据任务过去的运行表现动态调整优先级，同时又防止任务通过频繁 yield/syscall 作弊？**

进入 [第 15 课：让调度器根据运行表现调整优先级](15-mlfq.md)。把 timer 切换轨迹、普通 syscall 不重置时间片的证据和边界测试结果写进 [进度记录](progress.md)。