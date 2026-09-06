# 第 13 课：给内核接上时钟

状态：待开始。前置：[第 12 课](12-yield.md) 验收完成。分 A、B 两次，每次 45～60 分钟。

## 这课只解决“未来自动回来”

第 12 课只有用户主动 `yield`，内核才能换任务。

本课先不做任务切换，只回答：

> **内核怎样预约一个未来时间点，让正在运行的用户程序即使没有 syscall，也会自动 trap 回 S-mode？**

第一版只运行**一个用户任务**。timer trap 后仍恢复这个任务。第 14 课才把 timer 接到调度器。

参考：

- [SBI TIME 扩展](https://github.com/riscv-non-isa/riscv-sbi-doc/blob/master/src/ext-time.adoc)
- [RISC-V Supervisor 规范](https://docs.riscv.org/reference/isa/priv/supervisor.html)

## 先分清三套“调用/编号”

到这里已经出现三种完全不同的机制：

```text
普通函数 call
  → RISC-V ABI

U-mode ecall
  → neonos 自定义 syscall ABI
  → S-mode 内核

S-mode ecall
  → SBI ABI
  → OpenSBI / M-mode firmware
```

SBI TIME 的编号**不是** neonos `putchar=1` 那套用户 syscall 号。

SBI v0.2+ 调用约定中，S-mode 通常用：

```text
a7 = extension ID (EID)
a6 = function ID (FID)
a0..a5 = 参数
返回 a0 = error, a1 = value
```

TIME 扩展：

```text
EID = 0x54494D45
FID = 0 (set_timer)
```

RV64 下 `set_timer` 的绝对时间值放在 `a0`。

---

## A 次：预约一个 one-shot timer，并从单个 U-mode 任务接住它

### 第一步：先探测 TIME 扩展

计划创建：

```text
src/sbi.rs    → SBI 调用封装
src/timer.rs  → 课程自己的时间/期限逻辑
```

先通过 SBI BASE 的 extension probe 确认 TIME 可用。底层 `ecall` 返回的 SBI error 必须检查；不能因为 QEMU/OpenSBI 通常支持，就把所有错误当成功。

不支持时清楚输出：

```text
TIME extension unavailable
```

并终止本课 timer 实验，不伪造 fallback 成功。

### 第二步：知道 time 的单位从哪里来

`time` 是平台时间计数，不等于：

```text
CPU instruction count
宿主机毫秒
QEMU 输出行数
```

如果要把“10ms”之类的人类时间转换成 tick，需要取得当前实验机的：

```text
timebase-frequency
```

优先从 QEMU 生成的设备树/已核对平台信息记录，不猜一个“大家都说 QEMU 是某个频率”的常量。

本课程可以在记录清楚来源后暂时把该值作为实验配置常量；通用 FDT 解析不属于本课。

### 第三步：读取现在时间

RISC-V `time` 计数通常可通过 `rdtime`/time CSR 读取，但 S-mode 是否允许访问仍受平台计数器权限配置影响。

如果本环境读取发生 trap，不要把它伪装成 timer 驱动 bug；记录 OpenSBI/QEMU 配置并解决计数器访问问题。

### 第四步：计算一个未来的绝对 deadline

```text
now = read_time()
interval > 0
deadline = now + interval
```

检查：

- interval 不为 0；
- 单位转换不会舍入为 0；
- 加法不会静默溢出；
- 传给 SBI 的是**绝对时间点**，不是“再过 N tick”的相对值。

### 第五步：先 set_timer，再开放 STIE

顺序：

```text
安装好可处理 timer 的 trap 路径
→ set_timer(deadline)
→ 检查 SBI 返回成功
→ enable sie.STIE
→ 进入/恢复单个 U-mode 任务
```

本实验用 U-mode 用户任务等待事件，不额外创建一套“在 S-mode 中可返回的 timer 测试 handler”。

这里有一个 RISC-V 特权级细节：当前执行级别低于 S-mode（即在 U-mode）时，S-mode interrupt 的全局资格不由 `sstatus.SIE=1` 这一条件单独决定；但对应中断源仍需要 `sie.STIE` 打开，并且平台/固件必须把该 timer interrupt 正确委派给 S-mode。

进入 S-mode handler 后硬件会让同级中断默认不嵌套，本课也不主动重新开启嵌套中断。

### 第六步：只接一次 timer

收到 trap 后先分类：

```text
scause.interrupt = 1
cause code = 5 (Supervisor timer interrupt)
```

它不是 U-mode ecall，所以：

```text
sepc 不加 4
```

第一次实验只：

1. 记录 timer 到达；
2. 屏蔽 STIE/取消当前 timer 影响；
3. 恢复原用户现场；
4. 用户继续原来的计算。

用户在寄存器和栈中放哨兵，timer 前后必须一致。

### A 次验收

- [ ] TIME 扩展探测和 SBI error 检查存在。
- [ ] deadline 使用已记录单位的绝对计数。
- [ ] trap 原因为 interrupt code 5，而不是 exception 8。
- [ ] timer 后 `sepc` 保持原用户指令位置语义，不按 syscall 规则加 4。
- [ ] 同一个用户任务继续运行，寄存器/栈哨兵正确。

---

## B 次：连续事件，但仍不调度

### 每次 handler 都重新预约

第一版使用简单策略：

```text
on timer:
    count += 1
    now = read_time()
    set_timer(now + interval)
    resume same user
```

这是一种 one-shot 重预约方式。它简单但允许漂移：handler 本身耗时越久，下一个 deadline 也越往后。

现在不要引入“严格周期追赶”算法；第 15 课再讨论调度预算和过期期限。

### 不在 timer handler 疯狂打印

串口很慢，打印本身会严重影响时间观察。

使用固定容量事件记录，例如只保存：

```text
count
first_time
last_time
少量 deadline/actual 样本
```

在安全结束点统一输出摘要。

如果缓冲区满：

```text
记录 dropped count
```

而不是继续无限写。

### 连续 10 次后明确关闭

测试达到 10 次：

```text
clear sie.STIE
set_timer(u64::MAX)
检查 SBI 返回
```

SBI TIME 约定把 timer 设到最大值可用于清除/避免近期 timer pending；同时关闭 STIE 明确表示内核不再接收该 S timer 源。

不要用：

```text
set_timer(0)
```

来“关闭”，因为 0 是早已过去的绝对时间点，可能立即保持/触发 pending。

### 为什么“10 次”不是 10 个完美周期

我们验证的是：

```text
handler 真正进入了 10 次
```

而不是声称：

```text
宿主机秒表精确经过 10 × interval
```

QEMU 调度、handler 延迟、宿主机负载都会影响观测时间。

## 常见问题

| 现象 | 先检查 |
| --- | --- |
| 从不进入 timer handler | TIME 支持、deadline 是否未来、SBI error、STIE、委派 |
| timer 立刻疯狂重复 | 是否把相对 interval 当成绝对 deadline，或没有重新设未来时间 |
| 一次 timer 后用户重复/跳指令 | 是否错误对 interrupt `sepc += 4` |
| handler 里再次 timer 嵌套 | 是否意外重新打开同级中断 |
| 时间换算差几个数量级 | timebase-frequency、tick/ms/cycle 是否混淆 |
| 停止后仍持续进入 | STIE 是否清掉、timer 是否设到未来最大值 |

## 最终验收

### 运行验收

- [ ] one-shot timer 能从用户执行中自动 trap 回 S-mode。
- [ ] cause 类型/编号正确。
- [ ] 连续重新预约至少 10 次稳定发生。
- [ ] 用户现场在多次 timer 后保持正确。
- [ ] 日志有界，不靠中断内大量 printf 证明成功。
- [ ] 关闭后不再收到新的本课 timer 事件。

### 理解验收

不看正文回答：

1. neonos 用户 syscall 和 SBI ecall 分别从哪个特权级发起、交给谁？
2. `set_timer` 参数为什么是绝对时间点？
3. `timebase-frequency` 解决什么问题？
4. timer interrupt 为什么不能按 `ecall` 规则 `sepc += 4`？
5. 为什么本课把 timer 接到单个用户任务，而不是马上调度多个任务？
6. 为什么关闭 timer 同时处理 STIE 和未来 deadline 更清楚？

## 下一课为什么自然出现

现在 timer 能在用户不配合时把控制权抢回内核，但 handler 总是把 CPU 还给原任务。

下一步只差把两条已经学会的机制连起来：

```text
timer trap
+
第 12 课的 Ready queue/context switch
=
抢占式调度
```

进入 [第 14 课：程序不让出，内核也能切换](14-preemption.md)。把 TIME 能力、timebase 来源、10 次事件摘要和关闭结果写进 [进度记录](progress.md)。