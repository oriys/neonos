# 第 25 课：孩子结束了，父进程怎样知道

状态：待开始。前置：[第 24 课](24-exec.md) 验收完成。分 A、B 两次，每次 45～60 分钟。

## 为什么先学 `wait`，再学 `fork`

`fork` 一旦实现，父子生命周期会立刻变复杂。

所以本课先用**内核测试管理器**创建两个已经有 parent/child 关系的 Process，专心解决一个问题：

> **子进程已经结束了，但父进程还没有读取退出结果时，内核应该保留什么、释放什么？**

第 26 课再把“测试夹具创建子进程”替换成真正 `fork()`。

---

## 先定义本课的状态语义

### 运行中的 Process

```text
Ready / Running / Blocked
```

### 用户执行已经结束，但结果还等父进程读取

```text
Zombie(status)
```

Zombie 不是“还在运行的僵尸程序”。它表示：

```text
用户代码已经不会再执行
大块运行资源可以释放
但最小身份/退出结果必须保留，直到父进程 wait/reap
```

### wait 后真正回收

```text
Zombie
  ↓ parent wait 成功
Reaped / slot 可复用
```

如果继续把 `Exited/Faulted` 作为内部 completion outcome，也可以在安全管理点转换成 `Zombie(CompletionStatus)`；课程关键是**“完成”和“可复用 Process slot”之间有一个 waitable 状态**。

## 退出状态编码

本课程继续使用一个 32-bit 状态：

```text
normal exit code 0..255 → status = code
user fault             → status = 0x0001_0000 | cause
```

当前课程使用的 RISC-V cause 编号远小于 16-bit 上限；编码时仍用 checked conversion，不把任意大整数静默截断。

这不是 POSIX `wait status` 格式，只是 neonos 教学协议。

---

## A 次：子先结束，父后 wait

## 第一步：Process 关系必须有稳定身份

Process 记录增加：

```text
pid / generation
parent pid（或 KernelManager）
completion/zombie status
```

PID 不可只等于“当前槽位下标”。否则：

```text
旧 child slot=2 被 reap
新 process 又占 slot=2
父亲手里旧 PID=2
```

可能错误命中新实例。

第一版可以使用：

- 单调递增 PID；或
- `{slot,generation}` 组合。

### 直接子进程才可 wait

本课接口：

```text
wait(child_pid, status_ptr)
```

只等待一个**指定的直接 child**。

不实现：

- wait any child；
- process group；
- signal；
- POSIX options。

非直接 child / 不存在 child：

```text
-5
```

## 第二步：child 结束后先脱离运行资源

child `exit/fault`：

```text
用户 trap
→ 回 management/kernel root
→ 确认不再使用 child user root/trap execution frame
→ 释放 child AddressSpace 等大块运行资源
→ 保留最小 Zombie 记录
```

Zombie 至少保留：

```text
pid/generation
parent identity
completion status
```

Process slot 暂时仍被占用，防止 PID/结果在父亲读取前消失。

如果当前固定 trap-stack 存储是按 Process slot 静态预留的，它不需要“free frame”，但从这一刻也不再作为运行栈使用；真正可复用该 slot 要等 reap。

## 第三步：实现 `copy_to_user`

第 22 课只有：

```text
copy_from_user
```

本课增加对称接口：

```text
copy_to_user(address_space, user_va, src_kernel_bytes)
```

它沿用同一套范围规则：

1. checked `[start,end)`；
2. 非零长度时 canonical；
3. 逐页 software walk；
4. leaf 必须 `U=1`；
5. 这次是内核**写用户内存**，所以必须 `W=1`；
6. PPN 必须属于当前 AddressSpace 合法 user RAM；
7. `SUM=0`，通过 trusted kernel RAM mapping 写物理 frame；
8. 跨页逐段完成。

`wait` 写 4-byte status，所以要验证完整 4 bytes 都可写；不能只检查 `status_ptr` 第一个字节。

## 第四步：Zombie wait 的提交顺序

父调用：

```text
wait(child_pid, status_ptr)
```

child 已 Zombie：

```text
确认 child 是直接子进程
→ 读取 zombie status 到内核局部值
→ copy_to_user(status_ptr, 4 bytes)
→ copyout 全部成功
→ 才 reap zombie / 释放 Process slot
→ a0 = child_pid
→ sepc += 4
→ 返回父进程
```

**copyout 成功之前绝不能 reap。**

否则：

```text
status_ptr 无效
→ copyout -3
→ child 已经被删了
→ 父修好指针再 wait 也拿不到结果
```

正确行为：

```text
bad status_ptr → -3
Zombie 原样保留
后续合法 wait 仍能成功
```

### A 次测试

1. child `exit(7)` → Zombie；父之后 wait → 得到 PID + status 7。
2. child user fault → Zombie；父得到 fault status。
3. status_ptr 指向只读页 → -3，Zombie 仍在。
4. 修正到 RW user buffer 再 wait → 成功。
5. 同一 child 再 wait → -5，因为已经 reap。

---

## B 次：父先 wait，child 后结束

这次真正引入：

```text
Blocked(WaitChild(pid))
```

## 一个重要模型：阻塞 syscall 不保留 Rust 调用栈

neonos 仍采用中央 scheduler，不让：

```text
sys_wait()
  ↓
在 Rust 栈里 sleep
  ↓
以后从函数中间恢复
```

本课的简单 retry 模型是：

```text
用户执行 wait ecall
→ 内核发现 child 还没 Zombie
→ 在一个不可抢占区间登记等待
→ Process = Blocked(WaitChild(pid))
→ 保留用户 TrapFrame 中原参数和原 sepc（仍指向这条 ecall）
→ 回 scheduler
```

child 完成后：

```text
Blocked → Ready
```

父下次被调度：

```text
sret 回原来的 ecall
→ ecall 再执行一次
→ 内核重新检查 child/status_ptr
```

这次如果条件满足，就正常 copyout/reap，最后才：

```text
sepc += 4
```

这个模型的前提是：**第一次决定 Blocked 之前没有产生不可重复的副作用。**

以后 pipe 部分读写会专门处理“已经产生部分副作用就不能从头重试”的问题。

## 防止 lost wakeup

错误顺序：

```text
父：检查 child 还活着
         ↓ 切换出去
子：exit，没有看到父已登记 wait
         ↓
父：现在才 Blocked
→ 永远睡下去
```

所以在单 hart、S-mode 不抢占的临界区内完成：

```text
再次确认 child 状态
→ 登记 wait reason
→ 标记 Blocked
```

child 转 Zombie 时也在一致的 scheduler/process-table 保护规则下：

```text
如果 parent 正 Blocked(WaitChild(this_pid))
→ 清除/消费等待登记
→ parent Ready
→ enqueue 一次
```

不要重复 enqueue。

### wake 不等于 wait 已经完成

唤醒只说明：

```text
“值得再检查一次条件”
```

父重新执行 ecall 后仍要重新：

- 查 parent/child relation；
- 查 Zombie status；
- 验证 status_ptr；
- copyout。

这样以后即使状态模型更复杂，也不会把一次 wake 当成无条件成功。

## 父进程先退出怎么办

本课不实现完整 Unix `init` 进程。

教学规则：

```text
parent 退出
→ 存活 child 的 parent 改成 KernelManager
→ 已 Zombie child 立即由 manager reap
→ 未来 child 结束时 manager 自动 reap
```

因此孤儿不会永久占 Process slot。

要明确：这是 neonos 教学政策，不是 POSIX 全套 reparenting 语义。

## Ready queue 空现在不再等于“全部结束”

第 14 课时：

```text
Ready empty + no Running
→ 所有任务 terminal
```

现在可能：

```text
所有任务都 Blocked 等待 child
```

所以 scheduler 必须区分：

```text
没有 Ready
但存在 Blocked
```

如果没有任何未来可改变条件的来源，就报告：

```text
dead/stalled wait graph
```

而不是输出“stage complete”。

第 28 课加入 UART 外部输入后，会第一次有“所有用户 Blocked，但未来 timer/input 能唤醒”的正常 idle 情况。

## 常见问题

| 现象 | 先检查 |
| --- | --- |
| bad status_ptr 后 child 消失 | 是否 copyout 前就 reap |
| 父永远不醒 | wait condition check 与 Blocked 登记之间是否有 lost-wakeup 窗口 |
| 父醒后重复入队 | wake 是否只做一次 Blocked→Ready |
| 父醒后直接返回旧 status | 是否重新执行/检查 ecall，而不是把 wake 当成功 |
| slot 很快耗尽 | Zombie 是否需要 wait、孤儿是否由 manager reap |
| Ready empty 就宣布完成 | 是否还有 Blocked process |

## 最终验收

### Zombie/reap

- [ ] child exit/fault 后大资源释放，但最小 Zombie 结果保留。
- [ ] direct-child identity + PID generation 正确。
- [ ] `copy_to_user` 跨页/U/W/ownership 规则正确。
- [ ] bad status_ptr 不吞 Zombie；修正后可再次 wait。
- [ ] 一个 Zombie 只能成功 reap 一次。

### Blocking

- [ ] child-before-wait 和 wait-before-child 两种顺序都成功。
- [ ] Blocked 不在 Ready queue。
- [ ] child 完成只唤醒匹配 parent 一次。
- [ ] 被唤醒的 wait 重新验证 condition/pointer。
- [ ] Blocked wait 不保留悬空 Rust 调用栈或长期 user pointer 引用。
- [ ] orphan 由 KernelManager 最终回收。

### 理解验收

不看正文回答：

1. Zombie 为什么已经“不运行”却还不能立刻删 Process 记录？
2. 为什么 status copyout 必须发生在 reap 之前？
3. `copy_from_user` 和 `copy_to_user` 的权限要求分别是什么？
4. Blocked wait 为什么保留 `sepc` 指向原 ecall？
5. lost wakeup 是怎样产生的？
6. wake 为什么只表示“重新检查”，不表示 syscall 已无条件成功？
7. 为什么 Ready queue 空以后不能再直接等于“全部任务结束”？

## 下一课为什么自然出现

现在 parent/child、Zombie、wait 都有了，但 child 还是测试管理器凭空创建的。

下一步才实现：

```text
fork()
→ 从当前 Process 复制出一个真正的 child
→ 父子都从同一条 fork ecall 之后继续
```

进入 [第 26 课：一次调用，父子两条执行路线](26-fork.md)。把两种 wait 时序、bad-pointer 重试和 Zombie 资源曲线写进 [进度记录](progress.md)。