# 第五阶段：从进程接口到小 shell

状态：课程已安排，学习待开始。入口：[总路线](README.md) · [进度记录](progress.md)。

目标：在第四阶段地址空间隔离之上，实现可失败但原子提交的 `exec`、可等待/reap 的父子生命周期、eager-copy `fork`、fd/pipe 阻塞 I/O，以及真正运行在 U-mode 的最小 shell。

阅读 OSTEP 第 5 章 Process API，并结合前面的调度、地址空间和 user-copy 课程。以下接口是 neonos 教学 ABI，不承诺 POSIX/Linux 二进制兼容。

## 前置

先完成第四阶段：

- AddressSpace build/destroy 所有权清楚；
- `copy_from_user` 可跨页验证；
- user/kernel page fault 分流稳定；
- scheduler 支持 Ready/Running/terminal；
- 单 hart S-mode 内核路径仍不可抢占。

本阶段会第一次加入 `Blocked` 和外部输入，因此“Ready queue 空”的语义会改变。

## 12 次学习安排

| 次数 | 课程 | 当次问题 | 当次成果 |
| --- | --- | --- | --- |
| 1 | [24A exec candidate](24-exec.md) | 怎样不碰旧程序先构建新映像 | 完整 CandidateImage，失败全回滚 |
| 2 | [24B exec commit](24-exec.md) | 成功后为什么不回旧 ecall | 同 PID 替换 image/context，安全 root 后销毁旧地址空间 |
| 3 | [25A Zombie/wait](25-wait.md) | 子结束后为什么还保留记录 | copyout 成功后才 reap，bad pointer 不吞状态 |
| 4 | [25B Blocked/wakeup](25-wait.md) | 父先等、子后退怎样不 lost wakeup | 原 ecall retry、Blocked→Ready、orphan manager |
| 5 | [26A fork candidate](26-fork.md) | 哪些复制、哪些重建 | 独立 root/user frames，父子 post-fork 返回值不同 |
| 6 | [26B fork rollback](26-fork.md) | 中途失败怎样不产生半个 child | Building→Ready 最后发布，资源/slot 回滚 |
| 7 | [27A fd/endpoint](27-pipes.md) | fd 为什么不是 pipe 本体 | pipe/fork/dup2/close/exit refs 可核对 |
| 8 | [27B pipe I/O](27-pipes.md) | 部分读写与坏指针怎样原子 | read peek→copyout→consume；write copyin→append |
| 9 | [27C EOF/close](27-pipes.md) | 最后一端关闭怎样改变等待条件 | drain 后 EOF、no-reader -8、waiters 正确唤醒 |
| 10 | [28A UART/idle](28-shell.md) | 所有任务阻塞时谁继续收键盘 | NS16550 状态读取、RX ring、S-mode idle timer |
| 11 | [28B shell command](28-shell.md) | 怎样把 fork/exec/wait 串成命令 | help/exit/hello/cat，child exec failure 可回收 |
| 12 | [28C pipeline](28-shell.md) | 两个 child 怎样正确共享 pipe | `hello | cat`、大数据阻塞、refs/children 全回收 |

## 阻塞 syscall 的统一模型

本阶段不挂起任意 Rust kernel stack。

只有在**尚未产生不可重复副作用**时才允许：

```text
检查条件
→ 登记 Blocked(reason)
→ 保留原 user args + sepc（仍指向 ecall）
→ 回中央 scheduler
```

被唤醒：

```text
Blocked → Ready
→ 重新 sret 到原 ecall
→ ecall 再执行
→ 重新验证 pointer/fd/condition
```

完成或返回错误才把 `sepc` 前移。

如果 read/write 已经传输了 `n>0` 字节：

```text
立即 return n
```

不能再 Blocked 后从头重试，否则会重复副作用。

## 生命周期不变量

### exec

```text
old image 一直有效
+
完整 candidate
→ commit
→ 才销毁 old image
```

PID/parent/scheduler identity 保留，user AddressSpace/context 替换。成功不返回 old ecall。

### wait

```text
child complete
→ Zombie(status)
→ 大资源可释放，PID/status 保留
→ parent copy_to_user 成功
→ reap
```

copyout 失败不消费 Zombie。

### fork

```text
Building child
→ 完整复制 root/user pages/context/relation
→ 最后 publish Ready
```

失败 child 对调度器/父进程不可见。

### fd/pipe

```text
fd entry → endpoint ref → shared pipe object
```

fork 增 ref、exec 保留 entry、dup2 复制 ref、close/exit 减 ref。pipe ring 本体不因 fork 复制。

## 新 syscall ABI

保留 1～4，新增：

| a7 | 接口 | 成功结果 |
| ---: | --- | --- |
| 5 | `exec(program_id)` | 替换当前 image，不回旧程序 |
| 6 | `wait(child_pid,status_ptr)` | 返回 reaped child PID，写 32-bit status |
| 7 | `fork()` | parent=child PID，child=0 |
| 8 | `pipe(pair_ptr)` | 写两个 32-bit fd，返回 0 |
| 9 | `read(fd,buf,len)` | 实际 bytes；pipe EOF=0 |
| 10 | `write(fd,buf,len)` | 实际 bytes |
| 11 | `close(fd)` | 0 |
| 12 | `dup2(src,dst)` | dst |

错误码：

```text
-1 unknown syscall
-2 invalid argument/length
-3 invalid user address
-4 resource shortage
-5 no such direct child
-6 no such program
-7 bad fd/direction
-8 pipe has no readers
```

每次 read/write 最多 256 bytes。先校验 fd/方向/长度；zero length 返回 0 且不访问 buffer。

wait status：

```text
normal exit 0..255
fault       0x0001_0000 | cause
```

PID 使用 generation 或单调不复用身份。

## UART/input 与 idle 的新增边界

QEMU `virt` 的 NS16550-compatible UART 地址/寄存器从当前 DTB/平台资料核对；不把 `0x1000_0000` 推广成通用 RISC-V 规则。

第一版不用 PLIC UART IRQ，而是 SBI timer polling：

```text
UART → kernel RX ring → console fd0 read → user buffer
```

所有 user task Blocked 时，scheduler 进入 S-mode idle：

- 保留 rx poll deadline；
- `sie.STIE=1`；
- 在 S-mode idle 按规范允许 S interrupt 全局 delivery；
- `wfi` 后每次重新检查 Ready/event state；
- timer manager 统一取 scheduling deadline 与 rx-poll deadline 的最早值。

## stage tests

计划：

```text
tests/process.sh
tests/shell.sh
```

必须覆盖：

- exec success/failure + frame baseline；
- Zombie bad-pointer retry；
- wait 两种先后时序；
- fork 中途 OutOfFrames rollback；
- parent/child same VA 不同数据；
- pipe partial >512-byte pattern；
- bad pointer 不改变 ring；
- EOF/no-reader/hidden writer fd；
- UART input Blocked→wake；
- shell prompt/unknown/line-too-long；
- `hello | cat` 和大 producer pipeline；
- timeout 状态报告，而不是只判超时。

## 阶段总验收

- [ ] exec candidate/commit 原子，PID 不变，成功不回 old code。
- [ ] Zombie 只在 copyout 成功后 reap；orphan 不泄漏。
- [ ] fork child 完整构造后才 Ready，失败无半成品。
- [ ] fd refs 在 fork/dup2/close/exit 中守恒。
- [ ] pipe partial/blocked/EOF/no-reader 语义正确，bad pointer 无 ring 副作用。
- [ ] shell 在 U-mode 通过 fd0/1 和 process syscalls 工作。
- [ ] S-mode idle 能靠 timer polling 接收 UART 并唤醒 Blocked shell。
- [ ] `hello | cat` 与 >512-byte pipeline 都结束且资源回基线。
- [ ] boot/user/scheduling/memory/process/shell 回归通过。

通过后进入 [第六阶段：线程、锁与同步](stage-06.md)。