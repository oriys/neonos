# 第 20 课：第一次开启分页

状态：待开始。前置：[第 19 课](19-page-tables.md) 验收完成。分 A、B 两次，每次 45～60 分钟。

## 为什么这是一个危险切换点

第 19 课的 page table 只是内存里的数据结构。

第 20 课第一次让 CPU 真正按照它解释地址：

```text
satp.MODE = Sv39
```

从这一刻开始，CPU 的：

```text
下一条取指
当前 sp
Rust 数据访问
trap entry
UART MMIO
page-table walker 本身
```

都必须在新地址空间里可访问。

所以本课原则是：

> **先把“切换后需要的一切”列成清单并用软件 walker 验证，再写 `satp`。**

本课先关闭用户任务派发和 timer，只保留最小内核路径。

---

## A 次：先构造一个可证明完整的 kernel root

## 第一条策略：先用 identity mapping

本阶段让重要内核虚拟地址和物理地址数值相同：

```text
VA == PA
```

例如内核代码本来在物理 `0x8020...`，分页后仍从虚拟 `0x8020...` 执行。

identity mapping 不表示“没有分页价值”：页表仍然可以施加 R/W/X/U 权限。

### 第一步：列出切换后必需访问的所有区域

从第 04/17 课真实边界生成清单，而不是凭记忆：

- `.text` / trap 汇编入口；
- `.rodata`；
- `.data`；
- 普通 `.bss`；
- boot/management stack；
- 所有当前仍可能使用的 user/trap 静态栈；
- frame allocator metadata；
- heap arena；
- page-table frames；
- 当前仍需访问的 RAM frame pool；
- UART MMIO page；
- 当前还会读取的 FDT/启动数据；
- 当前路径真正使用的其他设备 MMIO。

OpenSBI 自己的私有内存不需要因为“固件存在”就全部塞进 S-mode page table；M-mode 执行 SBI 时不使用 S-mode 的 `satp` 翻译。只映射 neonos 自己需要访问的物理资源。

### 第二步：权限按区域最小化

教学基线：

```text
kernel .text      → R-X, U=0
kernel .rodata    → R--, U=0
kernel .data/.bss → RW-, U=0
stacks            → RW-, U=0
allocator RAM     → RW-, U=0
page-table pages  → RW-, U=0
UART/MMIO         → RW-, U=0
```

不要映射：

```text
RWX
```

只因为“这样比较不容易 page fault”。那会把本课最重要的权限学习抹掉。

### 第三步：避免 broad RAM mapping 覆盖 kernel 权限

一个常见错误：

```text
先 map .text RX
然后又 map 整段 RAM RW
```

如果映射 API 允许覆盖，最终 `.text` 可能悄悄变 RW。

本课程应：

- 所有 kernel section 边界按页整理；
- disjoint region 分开映射；
- duplicate map 默认拒绝；
- “剩余可用 RAM”只映射不属于 image/特殊区域的页。

如果 `.text` 和 `.data` 恰好共享同一个 4 KiB 页，就无法在页级同时给它两套权限。应先调整 linker layout 让安全边界落在页边界，而不是假装 PTE 能区分同页内字节。

### 第四步：软件预检

在写 `satp` 之前，用第 19 课 `translate()` 检查：

```text
当前 PC 所在 code page → RX
当前 sp 所在 stack page → RW
stvec entry              → RX
UART page                → RW
root/table pages          → RW
frame allocator metadata → RW
```

并验证：

```text
kernel pages U=0
text 不可写
data/stack 不可执行
```

如果任何一项失败，本课此时就停止，不尝试“先开分页看看”。

### A 次验收

- [ ] 有完整映射清单，不靠“整个 RAM RWX”兜底。
- [ ] section/page 边界支持预期权限。
- [ ] current PC/sp/stvec/UART/page tables 全部 software translate 成功。
- [ ] kernel 所有 mapping U=0。

---

## B 次：写 `satp`，然后证明真的启用了 Sv39

### `satp` 里至少包含三类信息

RV64 Sv39：

```text
MODE = Sv39
ASID = 0   （第一版）
PPN  = root page table 的物理页号
```

注意：

```text
root 的“虚拟指针”
≠ satp 需要的 root physical page number
```

必须明确取得 root frame 的 PPN。

### 切换前保持环境简单

- timer/普通 interrupt 源关闭；
- 用户任务不运行；
- 当前执行在 identity-mapped kernel code；
- 当前 sp 在 identity-mapped kernel stack；
- `stvec` 已指向分页后仍可执行的 kernel fault entry。

先打印一个：

```text
before paging
```

### 写入后执行本地转换同步

页表内容已在内存中准备好后：

```text
write satp
sfence.vma x0, x0
```

RISC-V 明确规定：写 `satp` 本身不能代替页表更新所需的地址转换同步。`sfence.vma` 用来让当前 hart 的后续地址翻译与此前 page-table stores 建立正确关系，并清理相关旧翻译。

当前单核、ASID=0，所以本课直接做完整本地 `sfence.vma`；更细粒度和多核 shootdown 留到后续。

### 立刻做最小活性检查

成功切换后不要马上做大功能。依次：

```text
1. 输出 "paging enabled"
2. 读一个 rodata 常量
3. 修改一个 data/bss 测试变量
4. 调一个普通 Rust 辅助函数（证明 stack/call 正常）
5. 再输出当前 root/satp 摘要
```

如果 `paging enabled` 都看不到，先检查 current PC/sp/UART/stvec/root PPN，不要去调用户进程。

### 读回 `satp` 做配置检查

在仍正常运行时读回 `satp`，确认：

```text
MODE == Sv39
root PPN == 预期
ASID == 0
```

如果平台不支持写入的 MODE，规范允许写入行为与支持模式有关；不要仅凭“写过 csrw”就宣称分页开启。

### 做一个安全负例

单独测试模式中准备一页专用 kernel test page：

```text
先映射 → 正常读写
unmap → sfence.vma
再访问
```

预期 kernel page fault 诊断。

不要第一次负例就 unmap：

- 当前 stack；
- 当前 code；
- trap entry；
- UART。

否则故障处理本身也可能失去依赖，留下完全静默的黑屏。

### 修改活跃 PTE 后必须同步

分页开启后，如果：

```text
map / unmap / 改权限
```

影响当前 hart 可能已经缓存的地址翻译，必须执行符合范围的 `sfence.vma`，不能只改内存中的 PTE 就假设 CPU 立即忘掉旧 TLB 项。

第 21 课切换整个地址空间时也会继续使用这个规则。

## 常见问题

| 现象 | 先检查 |
| --- | --- |
| 写 satp 后立即无输出 | current PC/sp/UART/stvec 是否全部映射 |
| 软件 translate 正确但 CPU page fault | satp root PPN、PTE flags、canonical VA、A/D、sfence |
| text 竟然可写 | broad RAM mapping 是否覆盖了 RX leaf |
| data 调用正常但 trap 黑屏 | stvec/trap stack/console 路径是否完整映射 |
| unmap 后仍能访问 | 是否缺 `sfence.vma`，仍命中旧 translation |
| 以为分页已开但 satp MODE 不对 | 是否读回验证写入结果 |

## 最终验收

### 正常路径

- [ ] `before paging` 和 `paging enabled` 都出现。
- [ ] code/rodata/data/stack/UART 正常。
- [ ] `satp` 读回模式/root 正确。
- [ ] kernel W^X/U=0 权限符合清单。
- [ ] frame allocator/page-table walker 在分页后仍能运行。

### 负例

- [ ] 专用测试页 unmap + `sfence.vma` 后产生可解释 kernel page fault。
- [ ] 恢复测试 mapping 后普通启动正常。
- [ ] 测试脚本等待 `paging enabled`，不只匹配 `Hello kernel`。

### 理解

不看正文回答：

1. identity mapping 为什么仍然是真分页？
2. 为什么 current PC、sp、stvec、UART 缺一项都可能在切换后黑屏？
3. `satp` 要 root PPN 还是一个 Rust 指针？
4. 为什么写 `satp` 后还需要 `sfence.vma`？
5. 为什么 text/data 共享同一物理页会妨碍精确权限？
6. 为什么第一次 page-fault 负例不能拿 trap stack 自己开刀？

## 下一课为什么自然出现

现在只有一个 kernel root，所有用户任务仍共享同一套地址翻译。

下一步才实现真正的进程隔离：

```text
Process A root: same VA → frame A
Process B root: same VA → frame B
```

进入 [第 21 课：两个进程，同一个地址，不同的数据](21-address-spaces.md)。把开启前映射清单、satp 读回值和安全负例结果写进 [进度记录](progress.md)。