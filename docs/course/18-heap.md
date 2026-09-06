# 第 18 课：页里面的小对象怎么分配

状态：待开始。前置：[第 17 课](17-frames.md) 验收完成。分 A、B 两次，每次 45～60 分钟。

## 为什么 frame allocator 还不够

第 17 课只能：

```text
一次给 4096 bytes
```

但内核以后会需要很多更小、大小不同的对象。

本课用一个**独立固定 arena** 学习最基础的 variable-size allocator：

```text
按 size + alignment 分配
→ 分割 free interval
→ 释放
→ 相邻合并
→ 观察 external fragmentation
```

阅读 OSTEP 第 14、17 章。本课先用显式接口，不急着接 `GlobalAlloc`、`Box`、`Vec`。

## 为什么 heap arena 暂时和 frame allocator 分开

预留例如：

```text
64 KiB heap arena
```

并从第 17 课 Free frame 范围中明确排除。

这样本课能独立学习“不同大小区间管理”，而不同时调试：

```text
frame allocator
+
heap grow/shrink
+
GlobalAlloc
```

第一版的边界非常明确：

> frame allocator 和 heap allocator 绝不能同时认为自己拥有同一片物理内存。

---

## A 次：按 size 和 alignment 分配

### 用半开区间表示所有块

统一：

```text
[start, end)
size = end - start
```

初始化：

```text
free list = [整个 arena]
allocated = 空
```

free/allocated metadata 自己使用固定静态数组，不能在 heap allocator 内部为了记一条 free block 又调用 heap。

### 本课的输入规则

`alloc(size, align)`：

- `size == 0`：课程显式拒绝；
- `align == 0`：拒绝；
- `align` 不是 2 的幂：拒绝；
- 地址/大小计算溢出：拒绝；
- 没有连续区间：返回 OutOfMemory；
- metadata slot 不足：返回明确 MetadataFull。

这些是本课自定义显式接口规则。以后适配 Rust `GlobalAlloc` 时重新按其契约设计，不把教学 handle API 直接冒充标准 allocator。

### first-fit 到底做什么

对每个 free interval：

```text
free = [start,end)
aligned = align_up_checked(start, align)
alloc_end = aligned + size (checked)
```

只有：

```text
aligned >= start
alloc_end <= end
```

才可使用。

选择第一个可容纳的 interval。

### 一次分配最多产生三段

例如：

```text
原 free:
[start -------------------------- end)

对齐后申请:
[start -- prefix -- aligned ==== alloc ==== alloc_end -- suffix -- end)
```

分配后：

```text
prefix（若非空）仍 Free
[aligned,alloc_end) Allocated
suffix（若非空）仍 Free
```

不能因为 alignment 产生前缀，就把 prefix 永久丢掉。

### 失败必须“无副作用”

一个常见 bug：

```text
先从 free list 删除原块
→ 发现 allocated metadata 满了
→ 返回失败
→ 原 free block 丢了
```

所以在提交修改前先确认：

- 新 allocated record 有位置；
- prefix/suffix 需要的 free metadata 有位置；
- 所有 checked arithmetic 成功。

然后一次提交状态变化。

### 用不同 pattern 证明不重叠

测试：

```text
size=1
size=24
size=4096
align=1/8/16/4096
```

每个分配区间写不同模式，只写自己的 `[start,end)`；最后检查其他对象内容没被改变。

### A 次不变量

任何时刻：

```text
所有 Free 区间互不重叠
所有 Allocated 区间互不重叠
Free 与 Allocated 不重叠
所有区间都在 arena 内
```

由于本课规定：

- allocator 元数据放在 arena 外的固定静态数组；
- alignment 产生的 prefix/suffix 必须继续留在 free list；
- 本课没有额外隐藏 header 占用 arena 字节；

所以还应有一个更强的守恒式：

```text
sum(free sizes) + sum(allocated sizes)
== arena size
```

如果小于 `arena size`，说明有字节被 allocator 悄悄“弄丢”了；这在本课设计里不是允许的 alignment 开销，而是需要定位的 bug。

---

## B 次：释放、合并和碎片

### 为什么本课用 handle 释放

为了先把“谁拥有哪块”做清楚，本课为每个 allocation 保存 generation/handle，而不是只凭任意用户传来的地址猜是哪块。

释放只接受当前有效 allocated handle：

```text
Allocated(handle) → Free
```

拒绝：

- 无效 handle；
- 已释放 handle；
- 旧 generation handle；
- arena 外任意地址。

### free list 保持按地址排序

释放后按起始地址插入，再只合并**真正相邻**区间：

```text
left.end == right.start
```

才可以：

```text
[left.start, right.end)
```

如果中间还有 1 字节 allocated gap，就绝不能跨过去“合并”。

### 一个必须做的分割/合并实验

```text
alloc A
alloc B
alloc C
free B
alloc D (D 比 B 小)
free D
free A
free C
```

最终应该恢复成：

```text
[arena_start, arena_end)
```

一个完整大 Free interval。

如果 free bytes 对了但仍有很多碎片，说明 coalescing 不完整。

### 构造 external fragmentation

让：

```text
total_free_bytes >= request_size
```

但：

```text
largest_free_interval < request_size
```

此时分配应该失败。

这正是 external fragmentation：

> 空闲总量够，但没有足够大的连续区域。

记录两个指标：

```text
total free bytes
largest free interval
```

比只看“还剩多少内存”更能解释失败。

### 和 frame allocator 做最后隔离检查

运行第 17 课的 frame 检查，确认 heap arena 对应物理页始终是 Reserved，不会被 `alloc_frame()` 返回。

反过来 heap allocator 的任何 `[start,end)` 也不能跑出自己的 arena。

## 常见问题

| 现象 | 先检查 |
| --- | --- |
| 返回地址不满足 align | 是否只对 size 取整，而没对 start 对齐 |
| 分配几次后总 free 莫名减少 | alignment prefix/suffix 是否丢失 |
| `free + allocated` 小于 arena | 是否有某段字节未被任何记录覆盖 |
| 释放全部却不能恢复大块 | free list 是否按地址排序并合并相邻块 |
| total free 足够仍失败 | largest contiguous free 是否不足 |
| metadata 满后 arena 少一块 | 失败前是否已经修改原 free list |
| frame allocator 返回 heap 地址 | 两套 allocator 是否重复拥有同一物理区域 |

## 最终验收

### 运行/不变量

- [ ] 多种 size/align 返回合法、不重叠区间。
- [ ] checked arithmetic 覆盖上溢。
- [ ] metadata 不足和 memory 不足都是无副作用失败。
- [ ] prefix/suffix 没有被静默丢失。
- [ ] `sum(free) + sum(allocated) == arena size` 始终成立。
- [ ] 无效/double free 被拒绝。
- [ ] 释放全部后 arena 恢复成一个完整 free interval。
- [ ] external fragmentation 场景可重复解释。
- [ ] heap arena 与 frame allocator 不重叠管理。

### 理解

不看正文回答：

1. fixed-size frame allocator 和 variable-size heap 分别解决什么问题？
2. alignment 为什么可能产生 prefix？
3. 为什么 allocator 失败前要先确认 metadata 也有容量？
4. 为什么本课里 `free + allocated` 必须严格等于 arena，而不能只是小于等于？
5. total free 和 largest free block 为什么要同时看？
6. 两个 free interval 什么条件下才能合并？
7. 为什么当前 handle allocator 不能直接声称实现了 Rust `GlobalAlloc`？

## 下一课为什么自然出现

现在内核会管理物理 frame 和小对象，但 CPU 仍然直接使用当前地址，没有“VA→PA”翻译结构。

下一步要把第 16 课的纸上映射真正编码成 RISC-V Sv39 页表：[第 19 课：手工搭建一棵页表](19-page-tables.md)。把分割图、fragmentation 场景和最终 arena 恢复结果写进 [进度记录](progress.md)。