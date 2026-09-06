# 第三阶段：让多个程序轮流使用 CPU

状态：课程已安排，学习待开始。入口：[总路线](README.md) · [进度记录](progress.md)。

目标：从第二阶段的单用户顺序运行，走到主动切换、timer 抢占和规则明确的简化 MLFQ；同时能用响应/周转等指标比较策略，而不是只凭“看起来更流畅”。

阅读 OSTEP 第 7～9 章；第 6 章用于复习内核如何重新取得控制权。彩票调度只做可选模拟，多核调度留到进阶。

## 开始前检查

先完成 [第二阶段](stage-02.md)：

- U-mode/syscall/exit/fault 控制流稳定；
- 用户 TrapFrame 能正确恢复；
- `run_user`/KernelContext 能安全回管理栈；
- `tests/user.sh` 不只匹配早期输出。

本阶段在这些机制上扩展，不重新复制第二套用户入口。

## 10 次学习安排

| 次数 | 课程 | 当次问题 | 当次成果 |
| --- | --- | --- | --- |
| 1 | [11 调度模拟](11-scheduling.md) | “更好”用什么指标衡量 | 手算并模拟 FCFS/SJF/RR，事件顺序明确 |
| 2 | [12A 多任务与队列](12-yield.md) | 多个任务的现场放哪里 | 固定任务表、Ready queue、persistent context |
| 3 | [12B 主动切换](12-yield.md) | 怎样从 yield 后继续 | A/B 可交替，压力切换管理栈不增长 |
| 4 | [13A one-shot timer](13-timer.md) | 怎样预约未来自动 trap | SBI TIME + 单用户 timer trap，原因识别正确 |
| 5 | [13B 连续 timer](13-timer.md) | 怎样持续预约而不破坏现场 | 10+ 次事件、日志有界、关闭语义明确 |
| 6 | [14A 抢占 RR](14-preemption.md) | 不 yield 的任务怎么轮换 | timer + Ready queue，长计算任务可抢占 |
| 7 | [14B 边界与回归](14-preemption.md) | syscall/exit/fault 怎么和时间片共存 | syscall 不刷新 quantum，内核不做未支持的嵌套抢占 |
| 8 | [15A MLFQ 模拟](15-mlfq.md) | slice/allotment/boost 怎样定义 | 高频 yield 不能逃避累计配额 |
| 9 | [15B 内核 MLFQ](15-mlfq.md) | 怎样按真实用户执行区间计费 | `last_user_enter` + 剩余预算，最早 deadline 预约 |
| 10 | [15C 对比](15-mlfq.md) | RR 与 MLFQ 如何公平比较 | 同输入指标表，不预设赢家 |

## 本阶段共同不变量

### 调度对象

保持最多 4 个受控用户任务。每个任务有独立 user stack、trap stack 和 persistent user context。

```text
Ready queue 只含 Ready
同一 task 最多出现一次
最多一个 Running
Exited/Faulted 永不重新入队
```

### 所有切换回中央 scheduler

```text
user trap
→ Rust handler 返回 action
→ 汇编恢复管理 KernelContext
→ 一个中央 scheduler loop
→ 选择/dispatch
```

不在每次 yield/timer 里递归进入新的 scheduler Rust 调用栈。

### timer 与 syscall 的 `sepc` 规则不同

```text
已处理 U-mode ecall → sepc + 4
interrupt            → 保持 sepc
unknown user fault   → 本阶段结束任务，不擅自跳过
```

### 本阶段内核不支持嵌套 timer 抢占

U-mode 可以被 timer 打断；S-mode trap/scheduler/console 更新期间保持同级中断关闭。timer 到期可以推迟到安全边界，不宣称硬实时。

### RR 普通 syscall 不刷新 quantum

新 quantum 只在 scheduler 真正 dispatch 新一轮时产生。普通 syscall 返回原任务保持旧 deadline/预算，避免 syscall-heavy 任务作弊。

### MLFQ 按用户执行区间计账

每次返回用户记录 `last_user_enter`；每次从用户 trap 回来先扣实际 delta。不能只数 timer interrupt 次数。

## 阶段测试

计划产物：

```text
src/scheduler.rs
src/sbi.rs
src/timer.rs
experiments/scheduling.py
experiments/mlfq.py
tests/scheduling.sh
```

宿主机模拟脚本不使用默认裸机 Rust target；Python 脚本直接在宿主机运行。

`scheduling.sh` 必须等待最终标记，并检查：

- timer switch reason 真实存在；
- 每个任务完成/故障次数；
- 计算 checksum；
- Ready queue/终态不变量；
- 无意外 panic/kernel fault；
- 超时能打印当前 task/queue/deadline，而不是静默失败。

## 阶段总验收

- [ ] 手算 response/turnaround 与模拟一致。
- [ ] yield 后多个任务从自己的旧位置恢复，管理栈不持续增长。
- [ ] one-shot/连续 SBI timer 可复现，单位来源有记录。
- [ ] 两个不 yield 的长任务能被 timer 抢占完成。
- [ ] 普通 syscall 不获得完整新 RR quantum。
- [ ] MLFQ 频繁 yield/syscall 仍累计用量并按规则降级。
- [ ] periodic boost/最低队列 RR 有确定轨迹。
- [ ] RR/MLFQ 用同一输入比较，结论由数据支持。
- [ ] 既有 boot/user tests 和新增 scheduling test 全部通过。

进入第四阶段前，应能画出：

```text
用户执行
→ syscall/yield/timer/fault
→ trap
→ 保存 persistent context
→ scheduler policy
→ 选择任务
→ 恢复 context
→ sret
```

完成记录后进入 [第四阶段：让每个进程拥有自己的内存空间](stage-04.md)。