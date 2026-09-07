# neonos × OSTEP 学习路线

目标：通过 Rust + RISC-V 教学内核，理解并实现操作系统的虚拟化、并发和持久化核心机制，并在第一轮末完成一个范围明确、可验证的崩溃恢复实验。

教材入口：[Operating Systems: Three Easy Pieces 官方免费章节](https://pages.cs.wisc.edu/~remzi/OSTEP/)。章节对应按官网目录核对于 2026-09-06。本路线是 neonos 的配套实验设计，不是书中的官方内核实现教程；RISC-V、Rust、SBI、VirtIO 和设备实现细节需要额外查对应官方规范。

## 从这里开始

想像 MIT lab 一样自己补代码、跑测试：从 [neonos Labs](../../labs/README.md) 开始。第 01～15 课现已提供三个学生实验，主仓库代码保留作参考实现。

- 完全零基础：先完成 [第 00 课：开始之前，先认识这台“电脑”](00-foundations.md)。它只建立 CPU、地址、寄存器、ABI、Rust 裸机、ELF/QEMU/OpenSBI 的最小概念地图，不要求先学完整门课。
- 当前工程基础：已有第 01～15 课的可运行检查点（第 11 课为宿主机模拟）。运行方式、回归命令及实现边界见 [工程实现记录](implementation.md)；代码具备不代表课程已经掌握。
- 当前正式课程：**第 01 课，待开始**。入口：[从开机到 Hello kernel](01-boot.md)。
- 学习记录：[进度与复盘](progress.md)。代码存在、教材准备、学习者完成课程，是不同状态。

## 课程阶段

| 阶段 | 课程 | 主题 | 详细安排 |
| --- | --- | --- | --- |
| 0 预备 | 00 | CPU、地址、寄存器、ABI、Rust/ELF/QEMU 最小地图 | [00-foundations.md](00-foundations.md) |
| A 内核基础 | 01～05 | 启动、输出、panic、内存布局、trap | [stage-01.md](stage-01.md) |
| B 用户程序 | 06～10 | 进程、用户态、系统调用、exit、用户错误 | [stage-02.md](stage-02.md) |
| C CPU 调度 | 11～15 | FCFS/SJF/RR、yield、timer、抢占、MLFQ | [stage-03.md](stage-03.md) |
| D 内存虚拟化 | 16～23 | frame、heap、Sv39、分页、地址空间、page fault、TLB | [stage-04.md](stage-04.md) |
| E 进程接口 | 24～28 | exec、wait、fork、pipe、shell | [stage-05.md](stage-05.md) |
| F 并发 | 29～35 | race、thread、atomic、mutex、condvar、semaphore、deadlock | [stage-06.md](stage-06.md) |
| G 持久化 | 36～43 | virtio-blk、cache、文件系统、inode、目录、文件 API | [stage-07.md](stage-07.md) |
| H 崩溃恢复 | 44～47 | crash consistency、fsck、full-block redo journal、recovery | [stage-08.md](stage-08.md) · [统一协议](stage-08-protocol.md) |

## 建议节奏

每次 45～60 分钟、每周 2～3 次。47 个正式学习单元中，很多单元会拆成 A/B/C 多次实验；前七阶段已经超过 80 次学习安排，第八阶段还会增加故障注入和恢复调试。

因此：

- 已有 Rust/C、体系结构或 OS 基础：约 **5～8 个月**是合理起步区间。
- 真正零基础、按每周 2～3 次学习：更现实的是 **7～12 个月**。
- 分页、上下文切换、VirtIO、文件系统和恢复阶段卡几次很正常，不用用“课号”衡量速度。

课程的完成标准不是“看完文档”，而是：功能实际运行、失败场景可复现、回归检查通过，并且能用自己的话解释为什么。

## 每课怎么学

每课统一采用这条学习链：

1. **今天要解决什么问题**：先知道当前内核缺什么能力。
2. **只引入本课需要的新词**：复杂概念第一次出现时按需解释。
3. **先预测**：先写下自己认为会发生什么。
4. **读真实代码/规范证据**：不靠“教材说应该这样”。
5. **只改一个层次**：减少同时变化的变量。
6. **运行并观察**：保存真实输出。
7. **故意制造一个失败**：确认错误路径也可解释。
8. **恢复正常实现并跑回归**。
9. **回答理解题**：能用自己的话解释才算完成。

Rust 的引用、裸指针、trait、宏、所有权随实验补充。第一次遇到汇编，逐条解释寄存器、指令、ABI 和保存顺序。新功能开始前先确认上一课 checkpoint 正常。

## 第一轮边界

第一轮面向：

```text
QEMU virt
RISC-V RV64
单 hart
OpenSBI
串口交互
固定容量资源为主
```

用户程序最初内嵌在 kernel image 中；文件系统完成后再从 disk file 加载受控 NEX1 程序。

完成后应能演示：

- 启动并解释 kernel boot chain；
- 用户程序通过 syscall 运行/退出；
- 多任务 timer preemption；
- 每进程独立地址空间；
- thread 和同步原语；
- shell、pipe、fork/exec/wait；
- VirtIO block + filesystem；
- 文件跨正常重启保存；
- 无日志时稳定复现 crash inconsistency；
- 在课程定义的 durable-boundary fault model 中，用 redo journal 恢复已提交事务。

---

# 47 个学习单元

## A. 内核基础：01～05

| 课次 | 问题与实验 | 验收 |
| --- | --- | --- |
| [01](01-boot.md) | 从 `cargo run` 追到 `Hello kernel` | 能说明 Cargo/QEMU/OpenSBI/_start/UART 各负责什么 |
| [02](02-console.md) | 把 UART 输出分层成 console/format/macros | 字符串、整数、地址可复用输出 |
| [03](03-panic.md) | 让 Rust panic 留下诊断 | message/location 正确，能说明 boot test 覆盖边界 |
| [04](04-memory-layout.md) | 画内存地图并显式清普通 BSS | probe 证明清零代码真实执行 |
| [05](05-traps.md) | 接住一次 CPU illegal instruction | TrapFrame/CSR/ABI 布局正确，报告后稳定停住 |

里程碑 A：内核能说明“运行到了哪里、哪里出了错”。

## B. 第一个用户程序：06～10

| 课次 | 问题与实验 | 验收 |
| --- | --- | --- |
| [06](06-process.md) | Program 与一次 Process 实例有什么区别 | Ready→Running→Exited 最小状态机可解释 |
| [07](07-user-mode.md) | 第一次进入 U-mode | user/trap/management stack 分离，U ecall 回可信内核 |
| [08](08-syscalls.md) | 处理 syscall 再返回用户 | ecall `sepc += 4` 只用于已识别固定长度 ecall |
| [09](09-exit.md) | exit 后不再返回原用户程序 | KernelContext 安全恢复，重复运行无栈漂移 |
| [10](10-user-errors.md) | 用户 fault 不等于 kernel fault | Faulted 在需要时引入，fault 后其他受控程序继续 |

里程碑 B：一个受控 U-mode 程序可以请求 kernel 服务、返回、退出或被 fault 终止。

## C. CPU 调度：11～15

| 课次 | 问题与实验 | 验收 |
| --- | --- | --- |
| [11](11-scheduling.md) | 用模拟理解 FCFS/SJF/RR | response/turnaround 手算与程序一致 |
| [12](12-yield.md) | 保存任务现场，实现主动 `yield` | A/B 可交替前进且现场不串 |
| [13](13-timer.md) | 接 SBI TIME，建立持续定时事件 | timer 可反复进入并正确关闭 |
| [14](14-preemption.md) | 用 timer 驱动抢占 RR | 不主动 yield 的任务也能切换，syscall 不刷新完整 quantum |
| [15](15-mlfq.md) | 实现简化 MLFQ | slice/allotment/boost/actual-user-time 计账可解释 |

里程碑 C：用户程序能够被 timer 抢占，机制与调度策略分开。

## D. 内存虚拟化：16～23

| 课次 | 问题与实验 | 验收 |
| --- | --- | --- |
| [16](16-addresses.md) | 区分物理地址与虚拟地址 | 手工解释地址转换和 page offset |
| [17](17-frames.md) | 管理可用物理 frame | 不碰 reserved、耗尽/非法 free/zero reuse 可验证 |
| [18](18-heap.md) | 实现简单 heap | 大小/对齐、split/coalesce、严格 byte 守恒 |
| [19](19-page-tables.md) | 手工构造 Sv39 page table | leaf/non-leaf PTE、权限和三级查询正确 |
| [20](20-paging.md) | 建 kernel mapping 并开启分页 | code/stack/UART/trap 正常，satp/sfence 可验证 |
| [21](21-address-spaces.md) | 每 Process 独立 root | same VA 保存不同数据，root 生命周期安全 |
| [22](22-page-faults.md) | fault 诊断和 user buffer validation | bad pointer 返回错误或结束 user，不伤 kernel |
| [23](23-vm-simulation.md) | TLB 与 page replacement 模拟 | TLB miss ≠ page fault ≠ swap |

里程碑 D：Process 有真正地址空间隔离，kernel 可以安全处理 user pointer。

## E. 进程接口与 shell：24～28

| 课次 | 问题与实验 | 验收 |
| --- | --- | --- |
| [24](24-exec.md) | 替换当前 Process image | success 不回 old image，failure 保留 old program |
| [25](25-wait.md) | Zombie/wait/reap | 结构化 status exact copyout 成功后才 reap |
| [26](26-fork.md) | eager-copy fork | parent/child return 不同、memory independent、failure rollback |
| [27](27-pipes.md) | fd/pipe/partial I/O | 只验证 actual `n` bytes，Blocked/EOF/ref lifecycle 正确 |
| [28](28-shell.md) | UART input + U-mode shell | idle timer 能唤醒 stdin，`hello | cat` 与大 pipeline 正确 |

里程碑 E：可以从用户命令行启动和组合程序，并明确 structured copyout 与 stream partial-I/O 的不同语义。

## F. 并发：29～35

| 课次 | 问题与实验 | 验收 |
| --- | --- | --- |
| [29](29-races.md) | 可控重现 logical race | 不依赖 Rust UB，错误/修复都可重复 |
| [30](30-threads.md) | 一个 Process 多 Thread | shared AddressSpace、independent context/stack |
| [31](31-atomics.md) | atomic、compound invariant、irq boundary | atomic field ≠ atomic invariant |
| [32](32-mutex.md) | sleeping mutex | Blocked、FIFO direct handoff、pending completion 正确 |
| [33](33-condvar.md) | condition variable | atomic release+sleep、reacquire、while predicate |
| [34](34-semaphore.md) | counting semaphore | permit 守恒和 direct grant 正确 |
| [35](35-deadlocks.md) | 构造并修复 deadlock | 区分 resource-allocation graph 与 wait-for graph，并有 cycle 证据 |

里程碑 F：能重现、解释并修复典型同步错误，不把 timeout 本身当 deadlock 证明。

## G. 持久化：36～43

| 课次 | 问题与实验 | 验收 |
| --- | --- | --- |
| [36](36-disk.md) | 准备独立实验 disk | 区分 byte / 512-byte sector / 4 KiB fs block |
| [37](37-block-read.md) | modern virtio-blk 初始化与 READ | feature/queue/status 顺序正确，queue setup 后才 DRIVER_OK |
| [38](38-block-write.md) | WRITE 与 FLUSH | completion/readback/flush/reboot 四层证据区分 |
| [39](39-cache.md) | block cache | hit/pin/dirty/needs_flush/writeback failure 明确 |
| [40](40-format.md) | 定义 on-disk format | host mkfs/dump 与 kernel mount 解码一致 |
| [41](41-inodes.md) | inode/file content | cross-block/EOF/reservation rollback 正确 |
| [42](42-directories.md) | directory/path | lookup/create/readdir 和正常发布顺序正确 |
| [43](43-file-api.md) | file API、NEX1、shell | file 跨重启保存，disk program 可执行 |

里程碑 G：具有可持久保存的文件和目录，并能从 disk file 加载受控程序；但 Stage 7 明确**没有 crash atomicity**。

## H. 崩溃一致性：44～47

统一技术协议：[stage-08-protocol.md](stage-08-protocol.md)。Stage 8 不允许逐课各自发明 commit/crash 语义。

| 课次 | 问题与实验 | 验收 |
| --- | --- | --- |
| [44](44-crash-consistency.md) | 无 journal 时把 update 拆成多个 durable step | 在 FLUSH-completed checkpoint 稳定复现 inconsistency |
| [45](45-fsck.md) | 只读 offline consistency checker | structural error 可定位；知道 fsck clean ≠ semantic commit correct |
| [46](46-journal.md) | full-block physical redo + staging | commit 前 home 不偷写；COMMITTED FLUSH 是唯一 commit point |
| [47](47-recovery.md) | boot recovery + durable fault matrix | PREPARED→old、COMMITTED→new，replay 中断仍幂等 |

Stage 8 每个 crash case 使用三种 oracle：

```text
journal state
+ structural fsck
+ semantic old/new state（file write 还要 exact bytes）
```

里程碑 H：在明确的 single-device / single-transaction / full-block / flush-boundary QEMU fault model 中，受支持的单个 filesystem mutation syscall 可以恢复到可验证的 old-or-new 状态。

不能把它宣传成：

```text
真实硬盘任意断电绝对不会坏
```

课程没有证明 torn 4 KiB write、device/controller 撒谎、多设备原子性、多核并发事务等问题。

---

## 可复现性约定

每个阶段结束都应保留：

```text
可复现命令
真实输出或摘要
成功标记
至少一个失败场景
对应回归测试
资源计数/边界检查（适用时）
```

随着课程实现推进，建议为稳定里程碑创建 Git tag，例如：

```text
lesson-05-done
lesson-10-done
stage-04-done
stage-08-done
```

这样后续复杂阶段出问题时，可以对比已知正确 checkpoint，而不是靠人工猜是哪一课开始损坏。

## 第一轮之后

第一轮不会把 OSTEP 所有章节都变成 kernel 功能。多核调度、真实换页、完整 VM、磁盘调度、RAID、LFS、SSD、distributed FS、安全和 VM/hypervisor 等保留为后续专题。

进阶实现顺序建议：

```text
copy-on-write
→ demand allocation + real swap
→ multicore synchronization/scheduling
→ stronger filesystem transaction/cache model
→ network + richer user runtime
```
