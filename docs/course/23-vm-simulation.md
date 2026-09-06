# 第 23 课：TLB 未命中和缺页不是一回事

状态：待开始。前置：[第 22 课](22-page-faults.md) 验收完成。建议拆成 A、B 两次，每次 45～60 分钟。

## 这课故意把两个层次分开

前面已经有真实 Sv39 page table。

现在会遇到两个很容易混淆的词：

```text
TLB miss
page fault
```

它们不是同一件事。

还会遇到第三件事：

```text
page replacement / swap
```

当前 neonos **没有真实 swap disk**。本课只用宿主机模拟理解算法，不把模拟结果说成“内核已经支持换页”。

阅读 OSTEP 第 19、21、22 章。

---

## A 次：只模拟地址翻译缓存 TLB

## 先画真实访问链

概念上：

```text
CPU 访问 VA
  ↓
查 TLB
  ├─ hit → 得到缓存的 PPN + permissions
  │
  └─ miss
       ↓
     page-table walk
       ├─ valid + permitted → 得到 PPN，填入 TLB
       └─ invalid/permission fail → page fault
```

所以：

> **TLB miss 只说明“翻译缓存里没有”，完全可能 page table 合法，然后正常 refill。**

不能写：

```text
TLB miss = 从磁盘把页面读回来
```

那把三层概念混在一起了。

## 一个 TLB entry 不只是 VPN→PPN

模拟 entry 至少保存：

```text
address-space id / ASID-like key
VPN
PPN
permissions
valid
```

因为即使 PPN 对了，缓存的权限也必须和当前映射一致。

当前真实 neonos 使用 ASID=0，每次 root switch 做完整 `sfence.vma`；模拟器可以做两个版本：

### 版本 1：context switch flush

```text
switch process
→ clear TLB
```

对应当前第一版 kernel 的思路。

### 版本 2：给 entry 加 process/ASID key

```text
(PID/ASID, VPN) → PPN + perms
```

观察不同地址空间的相同 VPN 可以同时存在而不混淆。

这只是模拟优化思想，不表示第 21 课已经启用真实 ASID。

## 固定序列

用 VPN 访问：

```text
1, 2, 1, 3, 1, 2
```

设置：

- page table 中 1/2/3 都合法；
- TLB 容量固定，例如 2；
- 替换规则明确，例如 FIFO 或 LRU，只为模拟翻译缓存。

每次输出：

```text
VPN
TLB hit/miss
是否 page-table walk
最终 PPN
权限检查结果
是否 refill/evict
```

### 两个守恒指标

对于所有需要翻译的访问：

```text
tlb_hits + page_table_walks = translation_attempts
```

如果某次 walk 最后 page fault，它仍然是一条 `page_table_walks`，只是没有 refill 成功。

不要把：

```text
page_table_walks
```

直接叫“page faults”。

## 做一个错误复用实验

两个进程：

```text
A: VPN 1 → PPN 100
B: VPN 1 → PPN 200
```

故意用错误 TLB key：

```text
只按 VPN=1 查
```

A 运行后切 B，不 flush，B 可能错误命中 A 的 PPN。

再分别用：

```text
context switch flush
```

和：

```text
(address-space, VPN) key
```

修复。

这个模拟正好解释第 21 课为什么 ASID=0 时每次 switch 都完整 `sfence.vma`。

## 权限也要进入 TLB 测试

例如先缓存：

```text
VPN 4 → R--
```

然后模拟一次 store。

即使 translation hit，也必须因为 cached permission 不允许 W 而失败；增加 TLB 容量不能“修复权限错误”。

### A 次验收

- [ ] 能画 TLB hit/miss → page walk → page fault 的三层关系。
- [ ] TLB entry 包含地址空间身份和权限，而不只是 PPN。
- [ ] 两进程同 VPN 不发生错误复用。
- [ ] `hits + walks = attempts` 守恒。
- [ ] permission failure 不被当成 cache capacity 问题。

---

## B 次：页面替换是另一个抽象层

### 本模拟重新定义一套模型

现在暂时不模拟 TLB，而是假设：

```text
虚拟 page reference
→ 某些 page 当前 resident in finite physical frames
```

如果访问的 page 不 resident：

```text
page fault（模拟）
→ 如果没空 frame，要选 victim
→ “从后备存储调入”新 page（纯模拟）
```

这里的“后备存储”没有连接 neonos VirtIO，也不会修改真实页表。

更准确的记录字段：

```text
reference page
hit / simulated page fault
victim
resident set
```

## 三种策略

### FIFO

淘汰最早进入 resident set 的 page。

命中时：

```text
不改变 FIFO 入队顺序
```

### LRU

淘汰最长时间没有被访问的 page。

每次 hit 也会更新“最近使用”顺序。

### OPT

淘汰“未来最晚才会再次使用，或以后不再使用”的 page。

它需要知道未来 reference string，所以是理论对照，不是当前真实内核可以直接实现的预测能力。

## Belady anomaly 固定算例

reference string：

```text
1,2,3,4,1,2,5,1,2,3,4,5
```

手算 FIFO：

```text
3 frames → 9 faults
4 frames → 10 faults
```

更多 frame 反而更多 FIFO fault，这就是经典 Belady anomaly 示例。

必须先手算前几步，再运行模拟器，不要只相信最终数字。

## 模拟器不变量

对于 replacement simulation：

```text
hits + faults = number_of_references
resident_count <= frame_capacity
victim 必须是 eviction 前 resident 的 page
hit 时不能凭空增加 resident_count
```

边界输入：

- 空 reference string；
- 1 frame；
- 单一 page 重复；
- 所有访问都不同；
- capacity <= 0 → 拒绝。

## 不要把“PTE V=0”直接解释成“page 在磁盘”

在一个真正支持 swap 的 OS 中，会有额外软件元数据说明：

```text
这个虚拟 page 是否存在一个 backing store copy
在哪里
是否允许重新调入
```

当前 neonos 没有这层。

所以第 22 课看到 unmapped/invalid PTE 时，正确行为仍是用户 fault；不会自动拿本课模拟器“从磁盘调页”。

## TLB 和 replacement 最后放到一张图

```text
            translation cache
VA ──→ TLB ─────────────┐
       miss             │
        ↓               │
     page table         │
        │               │
        ├─ mapping resident → PA
        │
        └─ page not present / invalid
              ↓
        [如果 OS 实现 demand paging]
        page-fault handler
              ↓
        replacement / backing store
```

当前 neonos 实际实现到：

```text
page table + fault diagnosis
```

TLB replacement 和 page replacement 都只做宿主机模型/概念实验；真实 CPU TLB 由硬件管理，本课程只用 `sfence.vma` 控制失效。

## 常见问题

| 现象 | 先检查 |
| --- | --- |
| TLB miss 都计成 page fault | 是否跳过了合法 page-table walk/refill |
| TLB hit 后写只读页也成功 | cached permissions 是否被忽略 |
| 两进程同 VPN 串映射 | key 是否缺 address-space/是否 context switch 未 flush |
| FIFO/LRU 总一样 | FIFO hit 时是否错误更新了顺序 |
| OPT 淘汰很快要用的页 | 是否选择了“未来最远”而不是“未来最近” |
| invalid PTE 被描述成“肯定在 swap” | 当前课程根本没有 backing-store 元数据 |

## 第四阶段最终验收

### TLB 模型

- [ ] hit / miss / walk / page fault 能明确区分。
- [ ] address-space key 与权限都参与 translation cache。
- [ ] ASID=0 + `sfence.vma` 的真实 kernel 设计能用模拟解释。

### Replacement 模型

- [ ] FIFO 3=9 / 4=10 fault 算例手算与程序一致。
- [ ] LRU/OPT 行为可复现。
- [ ] `hits + faults = refs`，resident size 不超容量。
- [ ] 明确知道 OPT 是 oracle、当前内核没有 swap。

### 理解验收

不看正文回答：

1. TLB miss 为什么不一定 page fault？
2. page-table walk 成功后 TLB 做什么？
3. TLB entry 为什么还需要 permissions 和 address-space identity？
4. ASID=0 时 context switch 为什么需要失效旧翻译？
5. FIFO hit 和 LRU hit 对内部顺序有什么不同？
6. 为什么 OPT 不适合直接当实际在线算法？
7. 当前 neonos 为什么不能看到 invalid PTE 就说“去磁盘换回来”？

完成 [第四阶段总验收](stage-04.md) 并更新 [进度记录](progress.md)。下一步进入 [第 24 课：进程不变，换一个程序运行](24-exec.md)。