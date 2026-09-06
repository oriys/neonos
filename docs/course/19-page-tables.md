# 第 19 课：手工搭建一棵 Sv39 页表

状态：待开始。前置：[第 18 课](18-heap.md) 验收完成。分 A、B 两次，每次 45～60 分钟。

## 本课目标

第 16 课只在纸上说：

```text
VA page → PA frame
```

本课把这条关系真正编码成 RISC-V Sv39 page table，但**暂时不写 `satp`，不让 CPU 使用它**。

先用软件 walker 验证结构全部正确，再在第 20 课开启分页。这样页表错误和“开启分页瞬间黑屏”不会同时出现。

阅读 OSTEP 第 18、20 章；架构位定义只以 [RISC-V Supervisor 规范](https://docs.riscv.org/reference/isa/priv/supervisor.html) 的 Sv39 为准。

---

## A 次：先能手工从 VA 走到 PTE

### Sv39 地址拆分

Sv39 用 4 KiB 基础页时，一个虚拟地址的低 39 位按：

```text
38          30 29          21 20          12 11           0
+--------------+--------------+--------------+--------------+
|   VPN[2] 9b  |   VPN[1] 9b  |   VPN[0] 9b  | offset 12b   |
+--------------+--------------+--------------+--------------+
```

所以：

```text
page offset = 12 bits
每一级 index = 9 bits = 0..511
每张 page table = 512 entries × 8 bytes = 4096 bytes
```

这就是为什么一张 Sv39 page table 恰好也是一个 4 KiB frame。

### canonical address 不能跳过

Sv39 并不是任意 64-bit 数字都合法。

地址高位必须是 bit 38 的符号扩展：

```text
bits 63..39 全部等于 bit 38
```

低地址用户布局自然满足这一规则；软件 `map/translate` 仍应显式拒绝非 canonical VA，而不是把高位静默截掉。

### 手算一个真实例子

继续第 16 课：

```text
VA page 0x00400000
→ PA frame 0x81000000
```

访问：

```text
VA 0x00400123
```

可以拆成：

```text
VPN[2] = 0
VPN[1] = 2
VPN[0] = 0
offset = 0x123
```

walker 路线：

```text
root[0]
  ↓ 指向 level-1 table
level1[2]
  ↓ 指向 level-0 table
level0[0]
  ↓ leaf → PA frame 0x81000000
+ offset 0x123
= PA 0x81000123
```

先手算，再让软件打印同样的三个 index。

## PTE 先只学本课用到的位

一个 PTE 是 64-bit。本课只需要先理解：

```text
V   valid
R   readable
W   writable
X   executable
U   user accessible
G   global（本课不用）
A   accessed
D   dirty
PPN physical page number
```

### 三种基本判定

#### Invalid

```text
V = 0
```

walker 停止：没有有效映射。

#### Non-leaf

本课程的中间表项要求：

```text
V=1
R=W=X=0
```

它的 PPN 指向下一张 page-table frame。

当前课程还规定：

```text
U=0
A=0
D=0
G=0
```

以及未实现的扩展位保持 0。RISC-V 当前规范对 non-leaf 的 U/A/D 等位有保留要求；本课程不使用 global subtree 优化，所以 G 也统一清零。

不要把用户 leaf 的 flags 原样复制到中间表项。

#### Leaf

如果 PTE 是有效映射叶子，本课只允许 level-0 的 4 KiB leaf。

至少有 R 或 X 使它成为 leaf；一个重要非法组合是：

```text
R=0, W=1
```

RISC-V 不把 write-only 这种组合当合法普通 leaf。本课映射入口直接拒绝非法权限。

### A/D 的课程策略

为了第一次分页实验不同时学习硬件/软件 A-D 更新策略：

```text
所有 leaf 预先 A=1
可写 leaf 预先 D=1
```

这样本轮不依赖访问时再修改 A/D。

这个规则只对 leaf；non-leaf 的 A/D 仍保持 0。

## 页表页本身怎么获得

每张页表从第 17 课 frame allocator 分配：

```text
alloc zeroed frame
→ ownership 记为 page-table page
```

必须清零，因为：

```text
全 0 PTE = V=0 = invalid
```

如果把上一所有者的旧字节当成 PTE，可能凭空出现“有效映射”。

软件访问 page-table frame 时，明确做：

```text
PPN → PhysAddr
```

再通过当前可用的可信物理/内核地址访问方式读写表页，不把“页号整数”直接 cast 成某个 Rust 引用而不解释地址关系。

---

## B 次：实现 map / translate / unmap

## `map_page`

输入概念：

```text
VirtPageNum
PhysPageNum
LeafPermissions
```

规则：

1. VA/PA 页对齐；
2. VA canonical；
3. permissions 是本课支持的合法 leaf 组合；
4. 从 root 按 VPN[2]→[1]→[0] 走；
5. 中间表不存在才分配 zeroed frame；
6. 遇到“本应 non-leaf 却已经是 leaf/格式非法”返回冲突；
7. 最终 leaf 已映射时默认拒绝，不静默覆盖。

### 失败必须回滚“本次新建”的中间表

如果：

```text
root 缺 level1 → 新建成功
level1 缺 level0 → 分配失败
```

应该：

```text
撤回刚挂进 root 的新 level1
释放本次新 frame
保留调用前所有旧映射
```

不要让一次 OutOfFrames 留下半棵空树。

## `translate`

软件查询只能读：

```text
VA
→ PTE path
→ PA + permissions
```

它**绝不分配 page table**。

所以一个很有价值的不变量：

```text
translate 前后 free_frame_count 完全相同
```

walker 遇到：

- V=0；
- 非 canonical VA；
- 非叶 PTE 格式非法；
- 不支持的大页 leaf；

都返回明确错误类型，而不是继续用错 PPN。

## `unmap_page`

本课语义只做：

```text
找到 level-0 leaf
→ 清除 PTE
→ 返回原映射的 PPN + permissions
```

**不自动 free 数据 frame。**

原因：

```text
撤销“这个 VA 能访问它”
≠ 已证明“再没有任何其他 owner/reference/map 使用这个 PA”
```

### 中间 page-table frame 怎么回收

第一版可以选择不在每次 `unmap` 时递归 prune 空表，而是让 PageTable 对象显式记录自己拥有的 table frames，并在：

```text
destroy_page_table
```

时统一释放。

这比写一半 prune 逻辑更容易证明所有权。

后续如果实现 prune，必须只释放：

```text
确实为空
且确实由当前 PageTable 独占拥有
```

的中间表。

## 小页池失败实验

限制 page-table frame pool，让一次 `map_page` 在创建中间层时故意耗尽。

验证：

```text
调用前已有映射仍能 translate
新 VA 没有半映射
本次新建 table frames 全回收
free_frame_count 回到调用前
```

## 常见问题

| 现象 | 先检查 |
| --- | --- |
| 同一个 VA 算出奇怪 index | 12/9/9/9 位移是否写错 |
| 非 canonical 高地址被静默映射 | 是否先截断再算 VPN |
| 中间 PTE 带 U/A/D | 是否把 leaf flags 复用到 non-leaf |
| `R=0,W=1` 仍能创建 leaf | 权限合法性检查是否缺失 |
| translate 让 free frames 变少 | 查询路径是否误调用“创建中间表”函数 |
| map 失败后 frame 泄漏 | 本次新建中间表是否完整回滚 |
| unmap 后另一个映射坏掉 | 是否错误把 data frame 自动 free |

## 最终验收

### 手算/结构

- [ ] 能拆出 Sv39 offset/VPN[0..2]。
- [ ] 能解释 canonical address。
- [ ] 能区分 invalid/non-leaf/leaf PTE。
- [ ] 能说明为什么 non-leaf 和 leaf 的 flags 规则不同。

### 软件页表

- [ ] 第 16 课 VA→PA 算例与 software translate 完全一致。
- [ ] map/translate/unmap 正常路径正确。
- [ ] duplicate mapping、非法权限、非 canonical、大页 leaf 均被拒绝。
- [ ] translate 无分配副作用。
- [ ] map 中途失败回滚 frame 与 PTE 修改。
- [ ] unmap 返回 mapping 信息，但不擅自 free data frame。
- [ ] PageTable table-frame ownership 有明确销毁规则。

## 下一课为什么自然出现

现在我们已经有一棵软件看起来正确的 Sv39 page table，但 CPU 仍然没用它。

下一问题是最危险的一步：

> **把 root 写进 `satp` 后，CPU 的下一条取指、当前栈、trap、UART 还找得到吗？**

进入 [第 20 课：第一次开启分页](20-paging.md)。把三级索引算例、失败回滚和 frame count 记录到 [进度记录](progress.md)。