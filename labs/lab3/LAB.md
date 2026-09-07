# Lab 3：谁运行，以及何时换人

阅读第 11～15 课。建议分 6～10 次完成。允许修改 `experiments/scheduling.py`、`experiments/mlfq.py`、`src/scheduler.rs`、`src/timer.rs`。

本 lab 将策略与机制分开：在宿主机实现调度模型，在内核实现 Ready 入队与 timer 启用。完整任务保存/恢复和内核 RR/MLFQ 循环已提供，要求阅读并解释其行为，不需要在这一版练习中重写整个调度器。

## Part A：FCFS/SJF/RR 模型，20 分

实现 `experiments/scheduling.py` 的 `simulate(jobs,policy='rr',quantum=2)`。

输入 Job(name,arrival,service)。时间为非负整数，名称唯一；未知策略、非正 quantum、负 arrival/service 应抛 ValueError。返回 `(result, trace)`：

- result：按名字索引，值包含 response、turnaround。
- trace：按执行顺序记录 `(name,start,end)`。
- 空输入返回空结果；service=0 在 arrival 时完成，两指标均为零。

SJF 非抢占；RR 同刻顺序：结算完成→新到达（输入顺序）→旧任务回队→选择。CPU 空闲时直接跳到下一到达。

```sh
make grade PART=model
```

A=6、B=2、C=1，同在 0 到达：FCFS 响应/周转总和 14/23，SJF 4/13，RR(q=2) 6/18。还必须通过新任务先于旧任务重新排队的边界例子。

## Part B：MLFQ 模型，20 分

实现 `experiments/mlfq.py` 的 `simulate(jobs,policy='mlfq',boost_period=40)`。本模型 T=2 个整数 tick；Q0 slice/allotment=2/4，Q1=4/8，Q2 slice=8、不再降级。RR 对照 quantum=2。

Job 的 burst=0 表示不主动让出；burst>0 表示每用满 burst 个 CPU tick 发起一次主动调度点；wait>0 为模拟 I/O 阻塞，wait=0 为 yield。阻塞时间不计用户用量。

返回字典：jobs（response、turnaround、max_ready_wait）、switches（派发次数）、events、trace。trace 每消耗一个 CPU tick 记录 `(tick,name,level)`；events 用 `(time,'demote',name)` 与 `(time,'boost',None)`。

要求：yield/阻塞不能清掉累计配额；按最高 Ready 优先级选择，同级保持队列顺序；新任务和唤醒按输入顺序；完成先于降级；到期 boost 最后执行并重置活跃任务预算，但不能将 Blocked 变 Ready。相同输入重复运行必须相同，总执行 tick 等于总 service。

```sh
make grade PART=mlfq
```

## Part C：Ready 队列，20 分

实现 `src/scheduler.rs` 中 `Ready::push(id)`，使用已有固定数组及 len。合法入队追加尾部，成功返回 Ok(())。id>=N 返回 Err("bad task id")；已满返回 Err("queue full")；尚未满但重复 id 返回 Err("duplicate task")。失败不能改变队列。

```sh
make grade PART=yield
```

检查真实 QEMU 的 ABABAB、单任务恢复、1000 次 yield、fault 后其他任务继续；用户自身检查寄存器、私有栈和计算结果。完整现场切换及 pop/check 已提供。

## Part D：启动定时器，20 分

实现 `timer::arm(deadline)`。deadline 是绝对平台 tick；先调用已提供的 SBI TIME 封装并确认成功，再启用 sie.STIE。不能在 S-mode 打开全局 SIE 造成内核重入。参考 sbi.rs 和课程第 13 课。

```sh
make grade PART=timer
```

验证一次 timer 和连续 10 次 timer，用户恢复后正确执行，最终关闭 timer。依赖 Part C。

## Part E：集成抢占与配额，20 分

没有额外 TODO，使用 C、D 与提供的调度器验证整体行为：

```sh
make grade PART=preemption
```

两个无 yield 长计算任务必须有 timer 导致的交替；普通 syscall 不重置完整 quantum；fault/exit 任务不再入队；MLFQ yield 不逃避配额。依赖 C、D；A、B 用于理解策略，不由内核调用。

评分使用 QEMU 确定性虚拟时钟；手动 cargo run 的宿主机时序不同，切换次数无需逐项一致。不要把某次切换次数硬编码进调度器。

## 完成标准

100/100，并在 answers.md 中说明模拟事件顺序、用户现场生命周期、syscall 与 timer 的 sepc 差异，以及 MLFQ 的两种预算。评分反映自动用例覆盖，不代表掌握多核、阻塞 I/O 或分页隔离。
