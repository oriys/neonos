# 第 17 课：管理可用的物理页

状态：待开始。前置：[第 16 课](16-addresses.md) 验收完成。分 A、B 两次，每次 45～60 分钟。

## 本课只管理固定大小的 4 KiB frame

第 16 课会算物理页，但还没有回答：

> **哪一页真的空闲？同一页会不会被重复分配？释放以后什么时候才能重新用？**

本课实现最小 frame allocator：

```text
4 KiB physical frame
→ 分配唯一所有权
→ 使用
→ 明确释放
```

不同大小对象和碎片问题留到第 18 课的 heap。

## A 次：先证明“哪些页能用”

### 先写一个保守原则

> **不确定是否空闲的物理内存，一律先当 Reserved。只有有证据的 RAM 区间才能标 Free。**

不要从：

```text
kernel_end 到 RAM_end 都应该能用吧
```

这种猜测开始。

### 第一步：取得实际 RAM 范围

根据当前 QEMU `virt` 配置、设备树和启动信息记录：

```text
RAM [start,end)
```

不要把 UART/virtio MMIO 等“有物理地址的区域”算成 RAM frame。

### 第二步：列完整预留清单

至少包含当前真正占用/仍会使用的：

- OpenSBI / firmware 占用区域；
- neonos kernel image 全部 section；
- boot/user/trap 等静态栈；
- frame allocator 自己的元数据；
- 第 18 课准备的固定 heap arena（若已预留）；
- 仍在使用的 FDT/启动数据；
- 其他明确保留的物理区间。

不要只看 `.text`；`.rodata/.data/.bss/stack` 都属于 kernel image/静态资源。

### 第三步：向内页对齐

对于一个候选可用区间：

```text
usable_start = align_up(start, 4096)
usable_end   = align_down(end, 4096)
```

只让完整落在范围内的页进入 allocator。

如果：

```text
usable_start >= usable_end
```

说明没有完整 frame，不产生任何 Free 页。

### 第四步：元数据先静态存在

计划在：

```text
src/memory/frame.rs
```

用固定容量 bitmap/状态表。

最容易证明正确的初始化方式之一：

```text
所有 tracked frame 初始 = Reserved
↓
只把“已验证 RAM 且减去全部预留”的 frame 改成 Free
```

而不是先把整个 RAM 标 Free，再希望自己没有漏掉某个保留区。

状态：

```text
Reserved
Free
Allocated
```

### 第五步：alloc 只做一种合法转换

```text
Free → Allocated
```

返回 `PhysPageNum/Frame`，不要用 0 地址当失败哨兵：

```text
Option<Frame>
或
Result<Frame, OutOfFrames>
```

同一 Allocated frame 不能再次返回。

### 内容初始化规则也写清

页的“所有权正确”和“旧数据是否泄露”是两件事。

教学版本可以统一：

> 每次分配成功后先把整个 4 KiB frame 清零，再交给调用者。

这样 page table/user page 都有确定初值，也直接避免上一所有者数据泄露。以后如需性能优化，再区分 raw/zeroed allocation。

### A 次检查

连续分配若干页，打印：

```text
frame number
physical base
free_before/free_after
```

自动检查：

```text
地址 4 KiB 对齐
不重复
不落进 Reserved
free_count 每成功分配一次减 1
```

---

## B 次：耗尽、释放和重新拥有

### free 只允许 Allocated → Free

释放前至少拒绝：

- 非页对齐地址；
- tracked RAM 之外；
- Reserved；
- 已经 Free 的 frame（double free）；
- 不属于 allocator 管理范围的 MMIO/任意整数地址。

错误不能改变计数和状态。

### allocator 不会自动知道“还有谁引用这页”

这是必须理解的所有权边界。

`free_frame(frame)` 的调用者必须保证：

```text
没有页表映射仍把它当有效数据页
没有设备 DMA 仍在访问
没有长期引用/指针仍在合法使用
```

frame allocator 只维护“谁拥有分配权”，不会扫描所有未来页表帮你做引用分析。

第 19/21 课会把“unmap”和“真正 free”分成两个动作。

### 用小测试池做耗尽

不要把整个实际 RAM 分光再期待内核还能正常运行。

从 allocator 中划一个受控小测试范围，例如 N 页：

```text
分配 N 次 → 全成功且唯一
第 N+1 次 → 明确 OutOfFrames
释放其中 1 页
再次分配 → 得到一个合法 Free frame
```

如果统一 zero-on-alloc，再先往该页写非零模式：

```text
old owner 写 0xAA...
free
realloc
新 owner 看到全零
```

证明重用没有泄露旧内容。

### 计数守恒

任何时刻检查：

```text
reserved_count + free_count + allocated_count
= tracked_frame_count
```

在“分配一批 → 全部释放”后：

```text
free_count == 开始时基线
```

如果失败操作发生，三个计数都不应变化。

### 可选 debug 技巧

以后调试 use-after-free 时，可以在 `free` 后先填 debug poison pattern，在下一次真正 `alloc` 交付前再清零。

这是调试扩展，不作为本课核心实现；不要为了 poison 破坏“交付给新所有者前清零”的保证。

## 常见问题

| 现象 | 先检查 |
| --- | --- |
| 分配几页后 kernel 莫名坏掉 | kernel/stack/FDT/metadata 是否真的 Reserved |
| UART 地址被 allocator 返回 | 是否把 MMIO 误算成 RAM |
| free 两次空闲数增加两次 | 状态是否检查 double free |
| 新用户页看到旧数据 | alloc 交付前是否清零 |
| unmap 后立刻 free 导致别处坏 | 是否还有共享映射/所有者引用 |
| allocator 初始化需要 allocator | 元数据是否错误地自举依赖自己 |

## 最终验收

### 运行/不变量

- [ ] RAM 来源和预留清单有真实依据。
- [ ] 只有完整对齐、确认可用的 RAM frame 标 Free。
- [ ] 分配唯一、耗尽有明确错误。
- [ ] Reserved/越界/double-free 全被拒绝且不改计数。
- [ ] 重用页交付前清零，非零 probe 对照通过。
- [ ] 完整分配/释放小池后 free_count 回到基线。
- [ ] 三类状态计数始终守恒。

### 理解

不看正文回答：

1. 为什么初始化时“默认 Reserved，再证明 Free”比“整个 RAM 先 Free”安全？
2. 为什么 MMIO 有地址但不能当普通 frame？
3. `unmap` 为什么不一定等于 `free`？
4. frame allocator 为什么不能自动知道所有页表引用？
5. zero-on-alloc 解决的是所有权问题还是信息泄露问题？

## 下一课为什么自然出现

现在只能按 4096 字节整页分配。

如果内核只想要：

```text
24 bytes
100 bytes
一个小对象
```

每次都占一整页会很浪费，而且需要解决不同大小、对齐、释放和碎片。

下一课：[第 18 课：页里面的小对象怎么分配](18-heap.md)。把真实 RAM/Reserved 图、小池耗尽和 zero-reuse 结果写进 [进度记录](progress.md)。