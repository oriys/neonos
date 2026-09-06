# 学习进度

课程入口：[neonos × OSTEP](README.md)。

- 当前：第 01 课，待开始；完全零基础先做 [第 00 课](00-foundations.md)。
- 已完成课程：暂无记录。
- 已有工程基础：仓库已经能启动并输出 `Hello kernel`；这不等于课程已经学会。
- 记录原则：**代码存在、教材已准备、学习者真正完成实验，是三种不同状态。**

## 里程碑

| 阶段 | 课程 | 教材状态 | 学习状态 | 验收记录 |
| --- | --- | --- | --- | --- |
| 0 预备 | 00 | 已准备 | 待开始 | — |
| A 内核基础 | 01～05 | 已准备 | 待开始 | — |
| B 用户程序 | 06～10 | 已准备 | 待开始 | — |
| C CPU 调度 | 11～15 | 已准备 | 待开始 | — |
| D 内存虚拟化 | 16～23 | 已准备 | 待开始 | — |
| E 进程接口与 shell | 24～28 | 已准备 | 待开始 | — |
| F 并发 | 29～35 | 已准备 | 待开始 | — |
| G 文件系统 | 36～43 | 已准备 | 待开始 | — |
| H 崩溃恢复 | 44～47 | 已准备 | 待开始 | — |

## 每阶段完成时必须保存什么

每个阶段不是“文档读到最后一页”就算结束。至少保存：

```text
运行命令
真实输出/摘要
成功标记
一个失败场景
回归结果
自己的解释
```

适用时再记录：

```text
frame/fd/process/thread/object 数量基线
地址范围
磁盘镜像/hash
故障注入点
```

复杂阶段建议保存 Git tag，例如：

```text
stage-01-done
stage-04-done
stage-07-done
stage-08-done
```

---

# 第一阶段：01～05

教材：[stage-01.md](stage-01.md)

| 学习单元 | 教材 | 状态 | 关键证据 |
| --- | --- | --- | --- |
| 01 启动链 | [01](01-boot.md) | 待开始 | cargo→QEMU→OpenSBI→_start→UART 可解释 |
| 02 console | [02](02-console.md) | 待开始 | UART address 与格式化层分离 |
| 03 panic | [03](03-panic.md) | 待开始 | message/location + deliberate failure |
| 04 memory/BSS | [04](04-memory-layout.md) | 待开始 | 真实地址图 + nonzero probe→clear→zero |
| 05 trap | [05](05-traps.md) | 待开始 | CSR/TrapFrame + Rust/asm offset 校验 |

---

# 第二阶段：06～10

教材：[stage-02.md](stage-02.md)

| 学习单元 | 教材 | 状态 | 关键证据 |
| --- | --- | --- | --- |
| 06 Program/Process | [06](06-process.md) | 待开始 | Ready→Running→Exited 最小状态机 |
| 07 U-mode | [07](07-user-mode.md) | 待开始 | user/trap/management 栈分离，cause=8 |
| 08 syscall | [08](08-syscalls.md) | 待开始 | ecall +4 边界、register sentinel |
| 09 exit | [09](09-exit.md) | 待开始 | 有效 exit 不回 user，100 次无栈漂移 |
| 10 user fault | [10](10-user-errors.md) | 待开始 | Faulted 在本课首次正式引入 |

---

# 第三阶段：11～15

教材：[stage-03.md](stage-03.md)

| 学习单元 | 教材 | 状态 | 关键证据 |
| --- | --- | --- | --- |
| 11 scheduling | [11](11-scheduling.md) | 待开始 | response/turnaround 手算与模拟一致 |
| 12 yield | [12](12-yield.md) | 待开始 | ABABAB + 大量切换栈不增长 |
| 13 timer | [13](13-timer.md) | 待开始 | TIME/cause=5/绝对 deadline/正确关闭 |
| 14 preemption | [14](14-preemption.md) | 待开始 | no-yield task 被切换，syscall 不刷新 quantum |
| 15 MLFQ | [15](15-mlfq.md) | 待开始 | slice/allotment/actual user delta/boost |

---

# 第四阶段：16～23

教材：[stage-04.md](stage-04.md)

| 学习单元 | 教材 | 状态 | 关键证据 |
| --- | --- | --- | --- |
| 16 addresses | [16](16-addresses.md) | 待开始 | VA/PA/page offset 两张地图 |
| 17 frames | [17](17-frames.md) | 待开始 | default Reserved→prove Free，zero reuse |
| 18 heap | [18](18-heap.md) | 待开始 | `free_bytes + allocated_bytes == arena_size` |
| 19 Sv39 | [19](19-page-tables.md) | 待开始 | 3-level/PTE/non-leaf invariant |
| 20 paging | [20](20-paging.md) | 待开始 | satp readback + sfence.vma |
| 21 AddressSpace | [21](21-address-spaces.md) | 待开始 | same VA→different PPN + safe destroy |
| 22 user copy | [22](22-page-faults.md) | 待开始 | canonical/U/R/W/ownership/cross-page |
| 23 TLB/replacement | [23](23-vm-simulation.md) | 待开始 | TLB miss ≠ page fault ≠ swap |

---

# 第五阶段：24～28

教材：[stage-05.md](stage-05.md)

| 学习单元 | 教材 | 状态 | 关键证据 |
| --- | --- | --- | --- |
| 24 exec | [24](24-exec.md) | 待开始 | candidate/commit，old image 未提前破坏 |
| 25 wait | [25](25-wait.md) | 待开始 | exact status copyout 后才 reap |
| 26 fork | [26](26-fork.md) | 待开始 | Building child 最后才 Ready |
| 27 pipe/fd | [27](27-pipes.md) | 待开始 | actual n 验证、peek→exact-copyout→consume |
| 28 shell | [28](28-shell.md) | 待开始 | idle UART poll、conditional close、>512B pipeline |

---

# 第六阶段：29～35

教材：[stage-06.md](stage-06.md)

| 学习单元 | 教材 | 状态 | 关键证据 |
| --- | --- | --- | --- |
| 29 logical race | [29](29-races.md) | 待开始 | Atomic+Barrier，无 Rust UB |
| 30 threads | [30](30-threads.md) | 待开始 | Process resource vs Thread state |
| 31 atomics | [31](31-atomics.md) | 待开始 | atomic field ≠ atomic invariant |
| 32 mutex | [32](32-mutex.md) | 待开始 | Blocked + FIFO direct handoff |
| 33 condvar | [33](33-condvar.md) | 待开始 | release+sleep atomic，reacquire before return |
| 34 semaphore | [34](34-semaphore.md) | 待开始 | permit direct grant + in-flight accounting |
| 35 deadlock | [35](35-deadlocks.md) | 待开始 | resource-allocation graph 与 wait-for graph 区分 |

---

# 第七阶段：36～43

教材：[stage-07.md](stage-07.md)

| 学习单元 | 教材 | 状态 | 关键证据 |
| --- | --- | --- | --- |
| 36 disk | [36](36-disk.md) | 待开始 | bytes/512B sector/4KiB block 区分 |
| 37 VirtIO READ | [37](37-block-read.md) | 待开始 | queue setup 后 DRIVER_OK、status/used-id/lifetime |
| 38 WRITE/FLUSH | [38](38-block-write.md) | 待开始 | completion/readback/flush/reboot 四层证据 |
| 39 cache | [39](39-cache.md) | 待开始 | hit/pin/dirty/needs_flush |
| 40 format | [40](40-format.md) | 待开始 | explicit offsets/LE/bitmap/layout |
| 41 inode | [41](41-inodes.md) | 待开始 | reserve all resources before mutation commit |
| 42 directories | [42](42-directories.md) | 待开始 | parser/lookup 分层，entry 后 publish size |
| 43 file API/NEX1 | [43](43-file-api.md) | 待开始 | fd/OpenFile/inode，checksum ≠ authentication |

---

# 第八阶段：44～47

教材：[stage-08.md](stage-08.md) · 技术协议：[stage-08-protocol.md](stage-08-protocol.md)

> Stage 8 只有一套 crash/commit 定义：**full-block physical redo + transaction staging + COMMITTED FLUSH 唯一 commit point**。

| 学习单元 | 教材 | 状态 | 关键证据 |
| --- | --- | --- | --- |
| 44 no-journal crash | [44](44-crash-consistency.md) | 待开始 | 仅在明确 durable checkpoint 下结论 |
| 45 read-only fsck | [45](45-fsck.md) | 待开始 | structural fsck ≠ semantic old/new oracle |
| 46 log/staging/commit | [46](46-journal.md) | 待开始 | commit 前 home cache 不偷写；PREPARED=old/COMMITTED=new |
| 47 recovery | [47](47-recovery.md) | 待开始 | T0/T1 old，T2/T3/T4 new；replay 中断仍幂等 |

Stage 8 每个 crash case 同时保存三类 oracle：

```text
journal state
fsck structural result
semantic old/new result（file write 还要 exact bytes）
```

不要把“fsck clean”单独当成 transaction 正确。

---

# 每次学习记录模板

完成课程后复制填写，不提前填入预期结果。

```text
课次与日期：
状态：学习中 / 已完成 / 需要复习
今天要解决的问题：
开始前预测：

修改了什么：
运行命令：
实际输出/摘要：
失败场景与实际结果：
回归检查：

我能用自己的话解释：
本课关键不变量：

还没弄懂的地方：
下次从哪里继续：
```

阶段完成时再额外记录：

```text
最终验收命令：
通过的回归测试：
保留的已知限制：
checkpoint/tag：
我现在能够解释的完整数据/控制流：
```

课程状态只按真实学习结果更新。教材已经准备完成，不等于学习记录可以提前标“已完成”。