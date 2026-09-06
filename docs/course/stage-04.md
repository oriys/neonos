# 第四阶段：让每个进程拥有自己的内存空间

状态：课程已安排，学习待开始。入口：[总路线](README.md) · [进度记录](progress.md)。

目标：从“所有任务共享同一个物理地址世界”走到完整的教学版 Sv39 隔离：物理 frame 有明确所有权，内核有可控 heap，页表能软件验证并真正启用，进程 root 可切换，用户非法访问可诊断，syscall user pointer 通过统一 copy 接口验证。

阅读 OSTEP 第 13～22 章；分段、TLB、页面替换通过图解/模拟学习。RISC-V PTE、`satp`、`sfence.vma`、page-fault cause 只以 [Supervisor 规范](https://docs.riscv.org/reference/isa/priv/supervisor.html) 为架构依据，不套用 x86 位定义。

## 开始前检查

先完成 [第三阶段总验收](stage-03.md)：多任务 persistent context、Ready queue、timer 抢占和调度边界稳定。

还要保留第一阶段真实 linker section/stack 边界。本阶段前半关闭 timer/用户派发调试内存基础；第 21 课地址空间切换稳定后再恢复 yield/RR/MLFQ 回归。

## 14 次学习安排

| 次数 | 课程 | 当次问题 | 当次成果 |
| --- | --- | --- | --- |
| 1 | [16 地址与内存地图](16-addresses.md) | VA/PA/page/offset 到底是什么 | 4 KiB 边界手算、地址类型和物理/虚拟两张图 |
| 2 | [17A 物理 frame](17-frames.md) | 哪些 RAM 页真的能分 | 默认 Reserved、证明 Free、唯一分配 |
| 3 | [17B 所有权/回收](17-frames.md) | 什么时候一页才允许重新用 | 耗尽、非法 free、zero-on-alloc、计数守恒 |
| 4 | [18A heap 分配](18-heap.md) | 小对象如何满足 size/alignment | first-fit + prefix/suffix，失败无副作用 |
| 5 | [18B heap 回收](18-heap.md) | 碎片与合并怎样发生 | handle free、相邻合并、external fragmentation |
| 6 | [19A Sv39 结构](19-page-tables.md) | VA 怎样走三级 PTE | VPN[2:0]、canonical、leaf/non-leaf 手算 |
| 7 | [19B 页表操作](19-page-tables.md) | map/unmap 失败如何保持所有权 | map/translate/unmap、回滚、table-frame 生命周期 |
| 8 | [20A kernel root](20-paging.md) | 启用分页前必须映射什么 | 权限最小化、PC/sp/stvec/UART software preflight |
| 9 | [20B 开启 Sv39](20-paging.md) | 写 satp 后怎样保证还能执行 | root PPN、`sfence.vma`、satp 读回、安全负例 |
| 10 | [21A Process AddressSpace](21-address-spaces.md) | 同 VA 如何映射不同数据 | 独立 root/user frames、kernel borrowed mappings |
| 11 | [21B root 切换/销毁](21-address-spaces.md) | TLB 与 frame 生命周期怎样安全切换 | ASID=0 全 flush、safe root 后 destroy、资源守恒 |
| 12 | [22A page fault](22-page-faults.md) | 非法取指/读/写怎样分类 | cause 12/13/15、user/kernel fault 分流 |
| 13 | [22B user copy](22-page-faults.md) | syscall 怎样安全读用户指针 | checked range、canonical/U/R(W)/ownership、跨页 copy |
| 14 | [23 TLB 与 replacement](23-vm-simulation.md) | TLB miss/page fault/swap 怎么分层 | TLB 模拟、Belady anomaly、FIFO/LRU/OPT 对比 |

## 本阶段共同设计

### 只做 4 KiB 页

不实现 huge page、真实 swap、demand paging 或 COW。第 19 课如果 software walker 遇到 level-1/2 leaf，当前教学实现明确报告“不支持的大页”。

### 三种所有权必须始终分开

```text
frame allocator
  → 物理页是否 Reserved/Free/Allocated

PageTable/AddressSpace
  → 自己拥有哪些 table/user frames

mapping
  → 某个 VA 能否访问某个 PPN + permissions
```

`unmap` 不自动等于 `free`；共享 kernel mapping 不等于每个 Process 都拥有 kernel physical frame。

### kernel 与 user mapping

第一版：

```text
kernel identity mapping → U=0
user code               → R-X,U=1
user data/stack         → RW-,U=1
stack guard              → unmapped
```

禁止 user W+X。

课程保持：

```text
SUM=0
MXR=0
```

内核不直接随意 dereference user VA。用户数据统一通过 `user_copy` software walk → trusted kernel RAM mapping 访问。

### 共享 kernel path 必须存在于每个 root

每个 Process root 都映射：

- kernel code/data；
- trap entry/当前 task trap stack；
- scheduler/management stack；
- page-table/frame allocator 需要的 RAM；
- console/UART。

这样 U→S trap 时可以直接在当前 root 下执行可信内核代码。

### ASID=0

本阶段所有 root 使用 ASID=0。每次地址空间切换：

```text
write satp(new root)
→ sfence.vma x0,x0
```

单 hart 下简单正确；ASID 优化和多核 TLB shootdown 留到进阶。

### user virtual layout

示例：

```text
code       0x0040_0000
private data 0x0100_0000
stack top  0x4000_0000
guard      stack 下 1 页 unmapped
```

地址只是课程虚拟布局，不是物理 RAM 位置。UART `0x1000_0000` 是 kernel U=0 MMIO mapping，user VA 规划避开它。

## 计划产物

```text
src/memory/frame.rs
src/memory/heap.rs
src/memory/page_table.rs
src/memory/address_space.rs
src/memory/user_copy.rs
experiments/vm.py
tests/memory.sh
```

如果已有 `src/memory.rs`，作为父模块声明子模块；不要同时创建冲突的 `memory.rs` 和 `memory/mod.rs` 结构。

## 阶段测试原则

`tests/memory.sh` 不能只看 `paging enabled`。至少覆盖：

- frame allocator Reserved/Free/Allocated 计数；
- heap split/coalesce/failure；
- software page-table map/rollback；
- `satp` 启用标记；
- dedicated kernel page-fault 负例（独立模式）；
- A/B same VA different PPN/data；
- root switch 多轮无 stale translation；
- instruction/load/store user page fault；
- `write_buf` 跨页/坏第二页/zero length/overflow；
- 最终资源 baseline；
- 旧 boot/user/scheduling 回归。

## 阶段总验收

### 物理内存

- [ ] 不确定的 RAM 默认 Reserved，allocator 不碰 kernel/stack/FDT/MMIO。
- [ ] alloc/free/zero/reuse/计数守恒可验证。
- [ ] heap 对齐、失败原子性、合并与碎片实验通过。

### 页表/分页

- [ ] Sv39 VPN/canonical/PTE leaf/non-leaf 能手算。
- [ ] map failure 回滚，translate 不分配，unmap 不擅自 free data frame。
- [ ] kernel root 权限不是 broad RWX，`satp`/`sfence` 规则正确。
- [ ] 分页开启后 code/data/stack/trap/UART/allocator 全部正常。

### 隔离/user copy

- [ ] A/B 相同 user VA → 不同 PPN，实际用户数据互不干扰。
- [ ] AddressSpace build/destroy 只回收 Owned frames，重复生命周期无泄漏。
- [ ] user page-fault cause 12/13/15 可解释，kernel fault 仍停机。
- [ ] user-copy 检查 canonical、权限、归属、跨页、溢出，坏指针返回错误不打坏内核。
- [ ] `SUM=0/MXR=0` 基线保持。

### 模型理解

- [ ] 能区分 TLB miss、page-table permission failure、模拟 page replacement。
- [ ] FIFO 3-frame/4-frame Belady anomaly 可手算。
- [ ] 明确当前 neonos 没有 swap/backing-store metadata。

进入第五阶段前，应该能画完整链：

```text
Process
→ AddressSpace/root
→ VA
→ TLB / page table
→ PPN
→ frame ownership
```

以及 syscall user pointer：

```text
user VA range
→ validation + page walk
→ trusted kernel copy
→ syscall side effect
```

通过后进入 [第五阶段：从进程接口到小 shell](stage-05.md)。