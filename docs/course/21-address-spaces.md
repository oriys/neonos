# 第 21 课：两个进程，同一个地址，不同的数据

状态：待开始。前置：[第 20 课](20-paging.md) 验收完成。分 A、B 两次，每次 45～60 分钟；第一次搬运用户代码时可以额外拆练。

## 这课终于实现“进程内存隔离”

第 20 课只有一个 kernel root。所有任务如果都使用它：

```text
VA 0x01000000
```

永远解释成同一个物理地址。

本课要变成：

```text
Process A root:
VA 0x01000000 → frame A

Process B root:
VA 0x01000000 → frame B
```

同时两棵 root 都保留相同的 kernel U=0 映射，使 trap、scheduler、allocator 在任何进程地址空间下都能工作。

---

## A 次：构造一个独立 AddressSpace

## 先写清“拥有”和“映射”不是一回事

每个 `AddressSpace` 至少记录：

```text
root frame
自己拥有的下级 page-table frames
自己拥有的 user code/data/stack frames
用户虚拟布局
```

而这些虽然会出现在每个 root 中：

```text
kernel .text/.data
scheduler stack
trap entry/stack
UART
allocator metadata
```

只是**共享的内核映射目标**。

它们不是“每个进程各自拥有一份物理页”。销毁 Process A root 时绝不能把 kernel code/UART/shared RAM frame 一起 `free`。

可以在设计里明确区分：

```text
OwnedFrame
BorrowedKernelMapping
```

即使代码类型没有这么命名，所有权语义也必须清楚。

## 用户虚拟布局先固定

沿用 stage 约定：

```text
user code   0x0040_0000 起
user data   0x0100_0000
user stack  top=0x4000_0000
stack 下方留 1 页 unmapped guard
```

第一版只做 4 KiB 页。

权限：

```text
user code  → R-X, U=1
user data  → RW-, U=1
user stack → RW-, U=1
guard      → unmapped
kernel     → U=0
```

明确拒绝用户 W+X 映射。

### `SUM` / `MXR` 基线也要固定

本课程第一版保持：

```text
sstatus.SUM = 0
sstatus.MXR = 0
```

含义先这样理解：

- S-mode 不直接把普通 user U=1 page 当成可随意解引用的数据来源；
- execute-only page 不因为 MXR 被扩大成可读。

第 22 课的 user-copy 会通过 software page-table walk 找到用户物理 frame，再通过 kernel 自己可信的 U=0 RAM mapping 访问。

这样“内核访问用户数据”是一个显式、可审计的入口，而不是临时开 SUM 后到处解引用 user VA。

## 用户程序不能直接把内核里的函数地址搬过去

当前用户样例原本链接在 kernel ELF 中。

要复制到低地址 user code frame，必须先把它整理成一个自包含 blob：

```text
user_start
...
user_end
```

内部：

- 分支/数据引用使用 PC-relative 或相对偏移；
- 不引用 kernel absolute symbol；
- 不依赖 kernel `gp`；
- 不直接 `call` kernel console/syscall dispatcher；
- 用户服务仍通过 `ecall`。

正式复制前查看反汇编/relocation，确认没有隐藏的绝对 relocation。

## 复制 user code

流程：

```text
alloc zeroed code frame(s)
→ 通过 kernel U=0 RAM mapping 写入 blob 字节
→ map 到目标 user VA 为 R-X,U=1
→ local fence.i
```

为什么 `fence.i`：CPU 可能已经有旧的 instruction fetch 观察状态；写完将要执行的新指令字节后，需要按平台规则做本 hart 指令同步。

user entry 不是旧的 kernel link address，而是：

```text
user_code_va + (user_entry_symbol - user_blob_start)
```

### 构造必须两阶段提交

`AddressSpace::build(...)`：

1. 分配 root/table/user frames；
2. 建 kernel borrowed mappings；
3. 建 user code/data/stack mappings；
4. 拷贝代码、初始化数据/stack；
5. software walk 检查全部权限和边界；
6. 全部成功才把完整 `AddressSpace` 交给 Process。

任何中间失败：

```text
只释放本次 Owned frames
不碰共享 kernel targets
不产生半个 Ready process
```

## 第一个隔离实验

A 与 B 都把：

```text
VA 0x0100_0000
```

作为私有数据页。

内核初始化：

```text
A frame 写 0xAAAA...
B frame 写 0xBBBB...
```

software walk 必须证明：

```text
A: same VPN → PPN_A
B: same VPN → PPN_B
PPN_A != PPN_B
```

用户执行时再读取/修改自己的数据，不能只靠内核 software walk 宣称隔离成功。

### A 次验收

- [ ] 每个 AddressSpace root/table/user frame 所有权清楚。
- [ ] kernel mapping U=0 且属于 borrowed target。
- [ ] user code/data/stack 权限符合规划，没有 W+X。
- [ ] `SUM=0, MXR=0` 是明确基线。
- [ ] 用户 blob 的 relocation/反汇编已核对。
- [ ] 相同 VA 在 A/B software walk 中得到不同 PPN。
- [ ] build 失败完整回滚 Owned frames。

---

## B 次：调度时切 root，并安全销毁旧地址空间

## 所有 root 中的 kernel 地址必须稳定

trap 发生时 CPU 当前仍在这个 Process 的 root 下。

所以每个进程 root 都必须把这些 kernel VA 映射到一致目标：

```text
trap entry
当前 thread/task trap stack
scheduler/management stack
kernel code/data
allocator/page-table walker RAM
console/UART
```

这样从 U-mode trap 到 S-mode 后，不需要先“神奇切 root”才能执行第一条内核指令。

`sscratch` 仍保存一个在当前 root 中可访问的可信 kernel VA。

## ASID=0 的第一版切换规则

当前课程所有地址空间：

```text
ASID = 0
```

所以 A→B context switch：

```text
保存 A user context
→ 选择 B
→ 写 satp(root_B, ASID=0, Sv39)
→ sfence.vma x0,x0
→ 准备 B sscratch/stvec/user state
→ sret
```

完整本地 `sfence.vma` 简单但正确。

不做 ASID 优化，也不做多核 TLB shootdown。

## 做一个专门抓 stale TLB 的实验

A/B 反复使用同一 user VA，但映射不同 PPN：

```text
A 写 A-pattern
switch B
B 写 B-pattern
switch A
检查仍是 A-pattern
...
```

主动 `yield` 先跑 100 次，再恢复 timer RR 跑 100+ 次。

如果漏掉 root switch / `sfence.vma`，很容易读到另一进程的旧 translation/data。

## 退出前必须先离开被销毁 root

一个 Process `Exited/Faulted` 后，不能在还使用它的 root 时马上：

```text
free root frame
free page-table frame
```

正确顺序：

```text
用户 trap
→ 回可信 trap stack
→ 回 scheduler/management stack
→ 切换到永久 kernel root（或另一个不会销毁的安全 root）
→ sfence.vma
→ 确认当前 hart 不再使用 dying root
→ destroy AddressSpace
```

单 hart 下做到这里即可；未来多核时还要考虑其他 hart 是否正在使用同一地址空间。

## destroy 只释放真正 Owned 的 frame

销毁：

```text
private user code/data/stack frames
owned page-table frames
root frame
```

不释放：

```text
shared kernel code/data frames
UART/MMIO
allocator global RAM targets
其他 process 的 frame
```

如果未来有共享用户页/COW，所有权模型要再次升级；当前第一版没有共享用户页。

## 重复创建/销毁的资源守恒

记录一个 baseline：

```text
free_frame_count
```

重复：

```text
build A/B
run
exit/fault
destroy
```

多轮后：

```text
free_frame_count == baseline
```

允许永久 kernel root/global page-table frames 作为阶段常驻资源，但必须在 baseline 前就计入，不能每轮缓慢泄漏。

## 常见问题

| 现象 | 先检查 |
| --- | --- |
| A 切回后读到 B 数据 | satp root、sfence、私有 PPN 是否真的不同 |
| 用户执行跳到 kernel 地址 | blob relocation/entry offset 是否仍使用旧 link address |
| 进入新 root 后 trap 黑屏 | trap/stack/console/kernel mapping 是否每个 root 都存在 |
| destroy A 后 B 崩溃 | 是否释放 shared kernel target 或 B 的 frame |
| frame count 每轮下降 | table/root/user frame rollback/destroy 是否完整 |
| kernel 直接 user VA 读取意外 fault | SUM 基线与 user-copy 访问方式是否按课程约定 |

## 最终验收

### 隔离

- [ ] A/B 同一 VA 对应不同 PPN。
- [ ] 用户实际读写自己的同 VA 数据，100+ 次切换不串数据。
- [ ] user code RX、data/stack RW、guard unmapped、kernel U=0。
- [ ] `SUM=0/MXR=0` 基线保持，内核不随意直接解引用 user VA。

### 生命周期

- [ ] AddressSpace 构造失败无 frame 泄漏。
- [ ] 退出先切安全 root，再 destroy dying root。
- [ ] 只释放 Owned frames，共享 kernel target 不被误 free。
- [ ] 重复创建/销毁后 frame count 回到基线。

### 回归

- [ ] 原 syscall/exit/fault 正常。
- [ ] yield/RR/MLFQ 切换正常。
- [ ] 每次 context switch 的 `satp/sfence` 规则可解释。

## 理解验收

不看正文回答：

1. “映射一个 kernel frame”为什么不等于“这个 Process 拥有这个 frame”？
2. 为什么每个 process root 都必须映射 trap/scheduler kernel 路径？
3. ASID=0 时为什么每次 root switch 都做完整本地 `sfence.vma`？
4. 为什么 user code copy 后需要 `fence.i`？
5. 为什么退出进程不能在自己的 root 正被使用时 free root？
6. `SUM=0` 的教学基线怎样影响下一课 user pointer 访问设计？

## 下一课为什么自然出现

现在用户被页表真正隔离了。

新的问题也出现了：用户可以把一个数字当指针传给 syscall，例如：

```text
write_buf(0xffff_ffff_ffff_ffff, 100)
```

内核绝不能直接信任这个地址。

所以下一课：[第 22 课：错误地址，也要有清楚的处理方式](22-page-faults.md)。把 A/B 同 VA 不同 PPN、切换轨迹和 frame baseline 写进 [进度记录](progress.md)。