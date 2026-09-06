# 第 22 课：错误地址，也要有清楚的处理方式

状态：待开始。前置：[第 21 课](21-address-spaces.md) 验收完成。分 A、B 两次，每次 45～60 分钟。

## 本课解决两个不同问题

分页开启后，地址错误会以两种方式出现：

```text
1. 用户自己执行了非法访存
   → CPU page fault
   → 当前 Process Faulted

2. 用户把一个坏指针作为 syscall 参数交给内核
   → 内核必须先验证
   → 返回错误码
   → 不能把内核也拖进 page fault
```

这两个场景必须分开。

本课不实现 demand paging、swap 或“缺页就自动补一页”。很多 page fault 本来就是权限错误，盲目补映射反而会破坏隔离。

---

## A 次：先把 page fault 分类清楚

### RISC-V 三类 page fault cause

在 `scause` 的 exception code 中，本课重点：

```text
12 = instruction page fault
13 = load page fault
15 = store/AMO page fault
```

它们和：

```text
instruction/load/store access fault
```

不是同一种 cause。

简单理解：page fault 主要说明地址翻译/页表权限阶段失败；access fault 可能来自更底层的物理访问/PMP/总线等问题。实验时按真实 `scause` 分类，不把所有“地址访问失败”都叫 page fault。

### page fault 报告至少记录

```text
pid / process id
root PPN / address-space id
scause
sepc
stval
software translate result
```

`stval` 通常帮助定位故障虚拟地址，但规范允许某些 trap 中信息有限；课程记录实际值，不写死“必须非零”。

### 五个受控用户实验

每个场景用一个新的用户实例，避免前一次 fault 状态污染后一次：

1. **load unmapped page** → 预期 load page fault 13。
2. **store 到 R-X code page** → 预期 store/AMO page fault 15。
3. **从 RW- data page 取指** → 预期 instruction page fault 12。
4. **U-mode 读取 kernel U=0 mapping** → 预期用户权限失败。
5. **访问 stack guard page** → 预期对应 load/store page fault。

用短小用户汇编构造，不使用 Rust UB。

每次 fault 后：

```text
Process → Faulted(info)
→ 回 scheduler
→ 再运行一个正常用户实例
```

证明受控用户 page fault 不等于 kernel failure。

### kernel page fault 仍是 kernel failure

单独测试模式中让 S-mode 访问专用 unmapped test page：

```text
kernel page fault
→ kernel diagnostic
→ stop
```

不能因为存在 current process 就把它记成用户 Faulted。

### A 次验收

- [ ] 12/13/15 三类 cause 能和取指/读/写场景对应。
- [ ] page fault 与 access fault 不混用名称。
- [ ] `stval/sepc/software walk` 能互相帮助定位。
- [ ] 五个用户场景都只终止当前实例，随后正常程序可运行。
- [ ] kernel page fault 独立报告并停住。

---

## B 次：建立可复用的 `copy_from_user`

### 为什么不能直接 `*(user_ptr)`

用户传：

```text
a0 = 一个整数地址
```

这个数字可能：

- 不是 canonical Sv39 地址；
- 完全 unmapped；
- 指向 kernel U=0 page；
- 指向只执行不可读的 page；
- 跨页后第二页非法；
- `start + len` 溢出；
- 指向 MMIO；
- 指向别的进程拥有的 frame。

所以“地址数值看起来在用户区”远远不够。

### 新增演示 syscall `write_buf`

课程协议：

```text
a7 = 4
 a0 = user start VA
 a1 = len
```

规则：

```text
len == 0       → 返回 0，不读取 start
len > 256      → -2
地址/权限非法   → -3
成功            → 返回 len
```

`write_buf` 最终把用户 buffer 的原始字节发送到 console，不要求是 UTF-8。

### 先设计通用 API，再让 syscall 调它

建议核心接口概念上是：

```text
copy_from_user(address_space, user_va, dst_kernel_slice)
```

以后 pipe/file/exec path 都可以复用。

本课最大长度固定 256，所以先准备一个固定 kernel buffer：

```text
[u8; 256]
```

不引入 heap dependency。

### 验证顺序

#### 1. 长度先判断

```text
if len == 0:
    return 0
if len > 256:
    return -2
```

零长度时不读取、不 canonical-check、也不 page-walk `start`。这是明确 API 语义。

#### 2. checked end-exclusive range

```text
end = start.checked_add(len)
```

失败 → `-3`。

范围统一：

```text
[start, end)
```

不是 `start + len - 1`，避免 `len=0` 下溢和边界混乱。

#### 3. start/end 涉及的 VA 必须 canonical

对非零长度，范围中的地址必须都属于合法 Sv39 canonical 地址空间；不能先截掉高位再查询。

#### 4. 按 page chunk 遍历

每次处理：

```text
current VA
→ 当前 page 的剩余字节
→ min(剩余请求, page 剩余)
```

跨页时进入下一 VPN，直到覆盖整个 `[start,end)`。

#### 5. 每个 page 都要逐项检查

software walk 需要得到一个合法 level-0 user leaf，并满足：

```text
V=1
U=1
R=1             （write_buf 要读取用户数据）
```

本课 `MXR=0`，所以 execute-only user page 不因为 X=1 就当可读。

此外还要验证 leaf PPN 对应的是**当前 AddressSpace 合法用户 RAM frame**，不能只是“PTE 有 U 位”就把任意 MMIO/错误物理地址当普通数据。

### 为什么不直接临时开 SUM

第 21 课基线：

```text
SUM=0
```

本课保持它。

流程：

```text
user VA
→ software page walk
→ 得到 user PPN + offset
→ 通过 kernel U=0 的可信 RAM identity mapping
→ 读取物理 frame 对应字节
→ 拷到固定 kernel buffer
```

这样所有 user pointer 访问集中在 `user_copy` 模块，更容易审计。

### 先完整 copy，再产生外部副作用

假设 200 bytes 跨两页：

```text
第一页合法
第二页 unmapped
```

错误实现：

```text
先把第一页 100 bytes 打到 UART
→ 第二页发现错误
→ 返回 -3
```

用户得到“失败”，但系统已经产生半份输出副作用。

本课要求：

```text
完整验证并复制到 kernel buffer
→ 全部成功
→ 才写 UART
```

因此 `write_buf` 是 all-or-error 输出。

### 当前 TOCTOU 为什么暂时可控

本阶段：

```text
单 hart
S-mode kernel 路径不被调度抢占
同一 address space 没有其他线程同时 unmap/remap
```

所以“验证 PTE → 读取 frame”期间映射保持稳定。

这是一条明确前提，不是永久真理。第 30 课加入同地址空间线程、未来加入 unmap/multicore 时必须重新设计锁定/pinning/reference 规则。

### 为以后提前定义 `copy_to_user` 对称规则

本课可以先只实现 `copy_from_user`，但把下一阶段会用的对称概念写下：

```text
copy_from_user → user leaf 要 U + R
copy_to_user   → user leaf 要 U + W
```

二者都要求 canonical、mapped、合法用户 RAM、checked range、逐页处理。

第 25 课 `wait(status_ptr)` 第一次真正实现 `copy_to_user`。

## B 次测试矩阵

| 场景 | 预期 |
| --- | --- |
| 同一页有效 buffer | 输出完整 bytes，返回 len |
| 正好到 page end | 成功，不多访问下一页 |
| 跨两页都有效 | 成功 |
| 第二页 unmapped | -3，UART 不出现第一页部分输出 |
| 指向 kernel U=0 | -3 |
| execute-only user page | -3（MXR=0） |
| non-canonical / checked_add overflow | -3 |
| len=257 | -2 |
| len=0 + 任意 start | 0，完全不碰 start |

## 常见问题

| 现象 | 先检查 |
| --- | --- |
| 单页工作，跨页 fault | page chunk/end-exclusive 遍历是否正确 |
| kernel pointer 居然被接受 | U bit / AddressSpace user-frame ownership 是否验证 |
| 第二页坏却先打印了一半 | 是否完整 copy 后才输出 |
| execute-only page 被读成功 | 是否意外依赖 MXR 或权限判断太宽 |
| len=0 仍 page fault | 是否在 zero-length return 前解引用/translate start |
| syscall 坏指针让 Process Faulted | 是否让 S-mode 直接 dereference user VA，而不是返回 -3 |

## 最终验收

### fault

- [ ] instruction/load/store page fault cause 可解释。
- [ ] user fault 与 kernel fault 分流正确。
- [ ] stack guard/U=0/RX/NX/unmapped 场景都有真实结果。

### user copy

- [ ] `copy_from_user` 按 checked `[start,end)` 逐页验证。
- [ ] canonical/U/R/user-RAM ownership 都被检查。
- [ ] `SUM=0/MXR=0` 基线保持。
- [ ] 跨页第二页失败没有部分 UART 副作用。
- [ ] zero-length 不访问指针。
- [ ] `tests/memory.sh` 覆盖以上矩阵，并保留已有 boot/user/scheduling 回归。

### 理解

不看正文回答：

1. page fault 12/13/15 分别是什么？
2. page fault 和 access fault 为什么不能混成一个词？
3. “VA 在用户数字区间”为什么不能证明可以读取？
4. 为什么 `write_buf` 要先完整复制再输出？
5. `SUM=0` 时内核怎样读取 user data？
6. 为什么当前 validate→copy 不发生 TOCTOU，但未来多线程/多核要重审？
7. bad syscall pointer 为什么应该返回 -3，而直接用户非法访存会 Faulted？

## 下一课为什么自然出现

页表已经能把 VA 翻译成 PA，但真实 CPU 不会每次访问都从 root 走完整三层表；它会缓存翻译结果。

另一方面，OSTEP 还讨论“物理内存装不下所有虚拟页时，淘汰哪一页”。这又是另一个层次。

下一课专门把两件容易混淆的事拆开：[第 23 课：TLB 未命中和缺页不是一回事](23-vm-simulation.md)。把 fault 报告和 user-copy 测试矩阵写进 [进度记录](progress.md)。