# 第 39 课：把常用磁盘块留在内存里

状态：待开始。前置：[第 38 课](38-block-write.md) 验收完成。分 A、B 两次，每次 45～60 分钟。

## 这课增加的不是“一个 HashMap”，而是新的持久化层次

现在每次读 4 KiB filesystem block 都可以直接发 8 个 sector 的 VirtIO request，但文件系统会反复访问：

```text
superblock
bitmap
inode table
目录块
```

所以加入固定 block cache：

```text
filesystem
  ↓
block cache（8 个 4 KiB slot）
  ↓
VirtIO block driver
  ↓
QEMU raw image
```

从这一课开始必须区分：

```text
cache 中已经改了
≠ device write 已完成
≠ flush 已完成
```

---

# A 次：命中、pin 和确定性替换

## cache key 必须包含设备身份

即使当前只有一个测试盘，也把 key 设计成：

```text
(device_id, fs_block_no)
```

而不是只用：

```text
block_no
```

以后如果挂第二个 block device，两个“block 8”不是同一数据。

## 一个 slot 至少保存

```text
valid
dirty
key
pin_count / ref_count
4 KiB data
replacement metadata
```

不变量：

```text
同一个 key 最多一个 valid slot
pin_count > 0 的 slot 不能被驱逐
invalid slot 不参与 hit
```

## `get_block(key)` 的基本流程

### hit

```text
找到 valid key
→ pin_count += 1
→ 返回受控 handle
```

### miss

优先：

```text
找 invalid slot
```

否则选择一个：

```text
pin_count == 0
```

的 victim。

如果 8 个 slot 全部 pinned：

```text
Busy / CacheExhausted
```

不能覆盖仍被调用者使用的数据。

## 替换算法先用确定性版本

不要一开始就实现复杂 LRU。

第一版可用：

```text
round-robin clock hand
```

只在 unpinned slot 中选下一个 victim。

固定输入得到固定 victim 顺序，方便调试 cache key/dirty/ref bug。

以后第 23 课学过 LRU 概念，但这不代表真实 cache 必须马上采用 LRU。

## fs block → sector 的转换只在一个地方做

```text
fs_block = N
sector_start = N * 8
sector_count = 8
```

使用 checked arithmetic，并验证：

```text
N < filesystem total blocks
sector_start + 8 <= device capacity
```

不要让 inode/目录代码自己到处重复 `*8`，否则很容易一处把 4 KiB block number 当 sector。

## miss read

victim 可用且 clean/invalid 后：

```text
read sectors N*8 .. N*8+7
→ 写入 slot.data
→ key=N
→ valid=true
→ dirty=false
→ pin_count=1
```

read device error：

```text
slot 不能被标成 valid 新 key
旧 victim 状态若尚未提交替换则保持原样
```

### 一个很重要的提交点

不要：

```text
先 slot.key = new_key
→ 再 device read
→ read 失败
```

否则下一次 hit 会把旧/未初始化 bytes 当成 new block。

建议先把 device read 到稳定临时 buffer，成功后再 commit slot key/data；或者严格安排 slot 状态为 Loading 且对外不可 hit。

## handle 的生命周期

返回的 cache handle 代表：

```text
这个 slot 当前被 pin
```

调用者结束访问必须 release：

```text
pin_count -= 1
```

当前课程规则：

> **不能跨 scheduler Blocked、user return 或长时间等待保留 cache handle。**

filesystem syscall 在单 hart、S-mode 不可抢占路径中完成一次有界 cache 操作，然后 release。

以后异步 I/O/多核会重新设计锁和 pin 生命周期。

## A 次实验

1. 连续读取同一 block 100 次：
   ```text
   device_reads = 1
   cache_hits = 99
   ```
2. 顺序读 9 个不同 block，容量只有 8：触发至少一次替换。
3. pin 住全部 8 个 slot，再请求第 9 个：得到 Busy，不破坏任何已有 slot。
4. release 一个后，第 9 个才能加载。

### A 次验收

- [ ] 同一个 key 不出现两个 valid 副本。
- [ ] hit 不触发 device read。
- [ ] pinned slot 从不被驱逐。
- [ ] miss read 失败不把 slot 错标成新 key。
- [ ] round-robin victim 轨迹可预测。

---

# B 次：dirty、writeback 和 `sync`

## `dirty` 到底表示什么

```text
dirty = cache 中的数据比当前已知成功写到 device 的 home block 更新
```

所以：

```text
cache 修改
→ dirty=true
```

不能在调用 `write()` 之前就提前 clear dirty。

## 驱逐 dirty victim

流程：

```text
victim dirty
→ WRITE 8 sectors 到 victim.key 对应 home block
→ 等 used completion + status OK
→ 只有成功后 dirty=false
→ 才允许换 key/reuse slot
```

如果 writeback 失败：

```text
key 保持旧值
data 保持旧 cache 内容
dirty 仍 true
slot 不被换成新 block
本次 cache miss 返回 I/O error
```

这样至少不会因为“驱逐失败”把唯一的新数据静默丢掉。

## `dirty=false` 仍不等于 durable

写回某个 block 成功以后：

```text
设备接受 WRITE completion
```

但本阶段还需要考虑 device/backend volatile cache。

因此 block cache 维护一个额外全局状态，例如：

```text
needs_flush
```

规则：

```text
任何 dirty block 成功写到 device
→ dirty=false
→ needs_flush=true
```

只有：

```text
FLUSH request success
```

才：

```text
needs_flush=false
```

### 为什么需要这个额外 bit

假设：

```text
所有 8 个 slot 都 writeback 成功
→ dirty slot 已经全部为 0
→ 但是 flush 失败
```

下一次 `sync()` 如果只看：

```text
有没有 dirty block？没有 → 直接成功
```

就会错误忘记上一轮尚未完成的 flush。

所以：

```text
no dirty
BUT needs_flush=true
```

下一次 sync 仍必须再次提交 FLUSH。

---

## `sync()` 的课程语义

第一版 global sync：

```text
1. 遍历 cache slots
2. 对每个 dirty slot：write home block，逐个检查成功
3. 全部 dirty writeback 成功后
4. 如果 needs_flush=true：提交 VirtIO FLUSH
5. FLUSH 成功 → needs_flush=false
6. return success
```

如果中间某个 block writeback 失败：

- 失败 slot 保持 dirty；
- 尚未处理的 dirty 继续 dirty；
- 之前已经成功写 device 的 slot 可以 clean，但 `needs_flush=true`；
- 本次 sync 返回错误；
- 不声称磁盘状态已事务回滚。

如果 flush 失败：

```text
所有 home write 可能已经到 device
needs_flush 保持 true
sync 返回错误
```

这就是第 38 课“写完成/flush 完成是不同层”的直接应用。

---

## 不允许 filesystem 绕过 cache 修改同一区域

一旦 filesystem block cache 生效：

```text
FS metadata/data read/write
→ 必须走 cache
```

如果另一条 FS 路径直接调用 block driver 修改同一 home block：

```text
cache 里仍保留旧副本
→ 下一次 hit 返回 stale data
→ 或未来 dirty writeback 把 direct write 覆盖掉
```

第 36～38 课的 probe driver test 使用独立 probe sectors/image；第 40 课 filesystem image 一旦 mounted，不允许宿主机在线修改，也不允许 FS 路径绕过 cache。

Stage 8 的 reserved journal blocks 会单独定义受控 direct-I/O/transaction 规则，因为它们不是 normal home cache key。

## dirty writeback 失败实验

用受控故障注入让某个 victim write 返回 IOERR：

```text
slot key = old block
slot dirty = true
slot data = 修改后的内容
```

然后恢复设备，重新 `sync()`：

```text
writeback 成功
flush 成功
重启读回正确内容
```

这比只测试 happy path 更能证明 dirty 状态不会提前丢。

## 常见问题

| 现象 | 先检查 |
| --- | --- |
| 同一 block 有时变回旧值 | 是否存在多个 cache 副本或绕过 cache 的 FS write |
| dirty victim write 失败后内容消失 | 是否提前换 key/clear dirty |
| sync 第二次没发 flush | 上次 flush 失败后 `needs_flush` 是否错误清零 |
| 所有 slot 永远 pinned | handle 是否跨返回/阻塞没有 release |
| 第 9 个 block 覆盖正在使用的数据 | victim 是否忽略 pin_count |
| host 修改 image 后 guest 仍读旧值 | mounted cache 没失效；不要在线 host 修改同一区域 |

## 最终验收

### cache

- [ ] 100 次同 block 只有首次 miss read。
- [ ] 超容量触发确定性替换。
- [ ] all-pinned 返回 Busy，无数据损坏。
- [ ] 同 key 唯一副本。

### dirty/writeback

- [ ] 修改后 dirty=true。
- [ ] writeback status OK 后才 clear dirty。
- [ ] writeback failure 保留 key/data/dirty。
- [ ] 成功 home write 会置 `needs_flush=true`。
- [ ] flush failure 后下一次 sync 仍会重试 flush。
- [ ] sync+reboot 能读回同一内容。

### 理解验收

不看正文回答：

1. cache hit 为什么不访问设备？
2. pin_count 解决什么生命周期问题？
3. dirty=false 为什么仍不一定 durable？
4. `needs_flush` 为什么不能只由“现在有没有 dirty slot”推导？
5. dirty eviction failure 为什么不能换成新 key？
6. filesystem 为什么不能一边用 cache、一边 direct-write 同一 home block？

## 下一课为什么自然出现

现在内核能缓存任意 4 KiB block，但还不知道：

```text
block 0 是什么？
哪些 block 空闲？
inode 放哪里？
根目录是哪一个？
```

下一课把“磁盘上一堆 bytes”变成一个明确、可由宿主机和 kernel 共同解码的 filesystem format：[第 40 课：约定磁盘上每个区域的含义](40-format.md)。