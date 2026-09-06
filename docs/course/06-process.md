# 第 06 课：程序与进程有什么区别

状态：待开始。前置：[第一阶段](stage-01.md) 验收完成。预计 45～60 分钟。

## 为什么先不急着进入用户态

第一阶段里，CPU 一直在执行 neonos 内核自己的代码。

下一步当然可以马上写 `sret` 进入 U-mode，但如果连“程序”和“这一次运行”都没有区分清楚，后面的退出码、调度状态、寄存器现场都会混在一起。

本课只解决：

> **一份程序代码，和“某一次正在运行的实例”为什么不是同一个东西？**

本课不进入 U-mode，不创建用户栈，也不实现调度。

## 阅读范围

阅读 [OSTEP 第 4 章 Processes](https://pages.cs.wisc.edu/~remzi/OSTEP/)，只抓住：

- process 是正在执行的程序实例；
- OS 需要保存它的运行状态；
- 同一个 program 可以产生不同 process。

第 5 章的 `fork/exec/wait` 只看接口目的，不在本阶段提前实现。

## 先用一个类比，但马上回到代码

```text
程序 program
≈ 菜谱

进程 process
≈ 按这份菜谱进行的某一次烹饪过程
```

两次都使用同一份菜谱：

```text
Program "hello"
   ├─ Process 1：正在运行，最后 exit=0
   └─ Process 2：另一次运行，可能还没开始
```

程序描述本身不会因为某次运行退出而变成 `Exited`。

## 三个东西必须分开

### 1. Program：静态描述

本阶段用户程序还没有来自磁盘，而是之后链接进内核镜像的一小段汇编。

可以先定义最小描述：

```text
Program
- name
- entry
- code_start
- code_end
```

这些字段回答“这是什么代码、从哪里开始”。它们不应该包含：

```text
当前寄存器
退出码
当前状态
```

### 2. Process：一次运行实例

本课只放当前真正需要的字段：

```text
Process
- id
- program
- state
```

不要为了“以后肯定会用”就提前把用户栈、陷入栈、页表、fd、调度队列全部塞进来。

它们在第一次真正需要时再加入：

```text
07 → 用户栈 / trap 栈 / 用户现场
10 → Faulted 终态与故障信息
21 → 地址空间
27 → fd
...
```

这能让每个字段的存在都有课程原因，而不是先得到一个看不懂的大结构体。

### 3. TrapFrame：CPU 某一时刻的现场

第一阶段已经做过 `TrapFrame`。

先这样区分：

```text
Process
= 一次运行的长期记录

TrapFrame
= CPU 在某个 trap 时刻的寄存器快照
```

未来一个 Process 会拥有或关联它自己的现场，但两者不是同义词。

## 本课新增一个 Rust 概念：enum 状态

用 `enum` 表示互斥状态。本课只定义当前真的会发生的三个状态：

```text
Ready
Running
Exited(code)
```

本课模拟：

```text
Ready → Running → Exited(0)
```

为什么现在不加 `Faulted`？因为第 06 课还没有运行真实用户代码，也没有用户 fault。第 10 课第一次真正需要“用户错误终止”时再加入 `Faulted(info)`。`Blocked` 也暂时不加入，因为现在没有任何等待 I/O/锁的机制。

这正好实践一个设计原则：**状态机不是一次猜全，而是随着真实行为出现再扩展。**

## 分步实验

### 第一步：创建 `src/process.rs`

只实现最小 `Program`、`Process`、`ProcessState`。

入口地址必须来自当前已链接的程序符号或明确的受控描述，不能凭空填写“看起来像代码地址”的整数。

### 第二步：集中处理状态转换

不要在各处随便写：

```text
process.state = ...
```

设计一个统一转换入口，让非法转换能返回错误。

本课规则：

```text
Ready   → Running      合法
Running → Exited(code) 合法
Exited  → Running      非法
```

如果要再次运行同一程序，应创建/重置成**新的进程实例**，而不是把旧的 `Exited` 强行改回 `Running`。

### 第三步：模拟一次运行

打印明确前缀：

```text
[model] pid=1 Ready
[model] pid=1 Running
[model] pid=1 Exited(0)
```

这里一定写 `[model]`，因为还没有执行任何用户指令。

不要把输出写成：

```text
user program exited successfully
```

那会把“状态机模拟”说成“真实运行”。

### 第四步：同一程序创建第二个实例

继续复用同一个 Program 描述，但给第二次运行新的 `id`：

```text
Program hello
  ↓
pid=1 ... Exited(0)

同一个 Program hello
  ↓
pid=2 ...
```

如果当前实现只有一个固定槽位，`id` 也不要简单等同于“槽位永远是 0”。至少使用单调运行编号，让日志能区分旧实例和新实例。

### 第五步：故意做一次非法转换

尝试：

```text
Exited(0) → Running
```

预期转换函数拒绝，且原状态保持 `Exited(0)`。

这个失败场景比只看 happy path 更能证明状态机真的约束了行为。

## 当前不要做的东西

本课明确不做：

- Ready queue；
- 多任务；
- 用户栈；
- `sret`；
- `fork`；
- `Faulted`；
- PID 回收策略；
- 堆分配。

如果现在就想做这些，先问：**本课的问题需要它吗？**

## 验收

### 运行验收

- [ ] 同一个 Program 描述可以用于两个不同 process id。
- [ ] `Ready → Running → Exited(0)` 输出符合模型。
- [ ] `Exited → Running` 被拒绝且不改变原状态。
- [ ] 原内核启动仍正常。

### 理解验收

不看正文回答：

1. Program 哪些内容应该稳定不变？
2. 退出码为什么属于 Process，而不是 Program？
3. Process 和 TrapFrame 的生命周期为什么不同？
4. 为什么第二次运行同一 Program 不应该简单“复活”旧 Exited 实例？
5. 为什么现在不需要 Ready queue？
6. 为什么 `Faulted` 要等第 10 课真正出现用户 fault 时再加入？

## 这一课结束后，下一问题自然出现了

现在我们已经有了一个“可运行实例”的数据模型，但它仍然只是 Rust 数据结构。

下一步才真正准备：

```text
Process 记录
  + 用户代码
  + 用户栈
  + 可恢复寄存器现场
  ↓
让 CPU 第一次进入 U-mode
```

进入 [第 07 课：第一次进入用户态](07-user-mode.md)。完成后把状态图和非法转换结果写进 [进度记录](progress.md)。