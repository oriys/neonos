# 已实现检查点与运行记录

本页记录工程实现和验证证据，不代表学习者已掌握对应课程；个人学习状态仍在 `progress.md` 中记录。

## 第 10～15 课

| 课程 | 实现与入口 | 验证 |
| --- | --- | --- |
| 10 | `cargo run --features lesson10-user-errors` | 用户 fault 后运行正常程序；非法参数返回；100 次运行；独立内核 fault/panic 会话 |
| 11 | `python3 experiments/scheduling.py` | FCFS/SJF/RR 手算结果、同刻事件、空队列、零工作量、晚到和无效输入 |
| 12 | `cargo run --features lesson12-yield` | ABABAB、私有栈/寄存器、单任务、1000 次 yield、fault 后继续、队列容量/重复项 |
| 13 | `cargo run --features lesson13-timer` | SBI TIME 探测；单次和连续 10 次 timer；原任务恢复；明确关闭 STIE |
| 14 | `cargo run --features lesson14-preemption` | 两个无 yield 计算任务被抢占且校验和正确；单任务、提前退出、fault、频繁 syscall |
| 15 | `cargo run --features lesson15-mlfq`；`python3 experiments/mlfq.py` | 按用量累计配额；yield 保留预算；降级/boost；相同负载 RR/MLFQ 对照 |

一次只选择一个用户执行/调度检查点 feature。默认 `cargo run` 保留正常启动及进程模型实验。QEMU 退出使用 Ctrl-A，再按小写 x。

### 完整测试

```sh
sh tests/boot.sh
for script in tests/lesson-*.sh; do sh "$script" || exit 1; done
sh tests/user.sh
sh tests/scheduling.sh
NEONOS_PROFILE=release python3 tests/user_batch.py
NEONOS_PROFILE=release python3 tests/scheduling_batch.py
```

模拟器测试入口为 `python3 tests/test_simulations.py`，已包含在 scheduling.sh 中。现有 CI 经 `tests/boot.sh` 调用 `tests/course.sh`，运行新检查点及优化构建；原工作流继续运行旧课回归。新检查点的完整本地日志保存在 `target/test-output/`，CI 控制台保留通过摘要或失败详情。

新 QEMU 检查不以 Hello/OK 早期输出作为结束条件：等待最终阶段标记，再收集一段输出，拒绝 panic/意外 trap/错误标记，并检查关键顺序、次数和进程存活。故障会话单独核对原因和来源，测试工具负责结束 QEMU。

### 实测证据

本地 QEMU virt 生成的设备树给出 `timebase-frequency=10000000 Hz`。调度代码使用 `T=10000` 个平台 tick，不猜测 CPU 指令频率；测试重新读取当前 QEMU 的设备树，保存到 `target/test-output/timebase.txt`。

第 11 课 A=6、B=2、C=1 的手算输入：

| 策略 | 响应时间总和 | 周转时间总和 |
| --- | ---: | ---: |
| FCFS | 14 | 23 |
| 非抢占 SJF | 4 | 13 |
| RR，quantum=2 | 6 | 18 |

第 15 课模拟对照使用完全相同的计算、晚到短任务、模拟 I/O、频繁 yield 负载。一次确定性运行得到 RR 68 次派发、MLFQ 56 次派发；这只描述该模型输入，不是普遍性能结论。完整输出由 `experiments/mlfq.py` 重现。

内核的 `response/turnaround/max_ready_wait` 使用平台 tick。切换事件只保存前 64 条，超出部分计入 dropped；任务自己的 timer/yield/syscall/抢占/降级计数仍完整保留。仅在批次结束后打印统计，主动 ABABAB 演示除外。

### 当前实现边界

- 单 hart、S-mode 不抢占；最多 4 个任务，分别拥有 16 KiB 用户栈和 trap 栈。
- 用户运行仍处于 Bare 地址模式。栈和代码范围检查用于受控实验，尚未提供页表隔离。
- 调度恢复通过同一个 KernelContext 返回桥，现场按值复制；不跨调度保留 Rust trap 栈引用。
- 第 13 课借助一个内核提供的共享完成标志结束受控用户循环。这不是通用用户内存接口；分页前的教学夹具在第 21 课需要替换。
- 第 14 课 RR 保持绝对 deadline；普通 syscall 不赠送新 quantum，过期可在安全返回点直接重新调度，因此应检查抢占数而不能只数 timer trap。
- 第 15 课在进入用户的汇编桥前和 trap handler 入口计时，排除主要内核分发时间，但包含保存/恢复现场的固定开销；不是硬件精确的纯用户 CPU 计费。
- MLFQ 的 Q0/Q1 slice=1T/2T、allotment=2T/4T，Q2 slice=4T；每 20T 提升活跃任务。频繁 yield 实验保留有限用户计算段，以便实际消耗可测的用户预算。
- I/O 阻塞和晚到负载目前在宿主机模拟中实现，内核本阶段只运行预先创建的 Ready 任务。空队列表示本批次已结束。

下一步按第 16～23 课实现地址模型、页分配、堆、Sv39 和地址空间隔离。
