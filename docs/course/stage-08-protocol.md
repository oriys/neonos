# 第八阶段统一协议：Full-block Redo Journal 与崩溃模型

这份文档是第 44～47 课共同遵守的**技术协议**。目的不是再增加一套文件系统功能，而是让四课对“什么叫 commit、什么能 replay、哪些 crash 结果可预测”使用同一个定义。

如果逐课教材和这里发生冲突，以这里为 source of truth，并立即同步修正文档；课程不允许同时保留两套 crash/commit 语义。

---

## 1. 第一轮到底保证什么

Stage 8 对当前受支持的**单个 filesystem mutation syscall**提供：

```text
full-block physical redo transaction
```

journal 记录本次事务中**所有会被修改的 filesystem home blocks 的完整 4096-byte 最终新镜像**，包括 regular-file data block。

因此在本阶段明确定义的 fault model 内：

```text
crash before durable COMMITTED
→ recovery 得到 transaction 前 old state

crash at/after durable COMMITTED
→ recovery 最终得到 transaction 后 new state
```

第一轮不是“metadata journal + ordered data”模式。那是另一种设计，会有不同的持久化语义和测试 oracle。

### 原子性范围是一个 syscall / transaction

例如：

```text
open(CREATE)     # 一个 create transaction
mkdir            # 一个 mkdir transaction
write <=256 B    # 一个 file-write transaction
```

shell：

```text
touch /a
write /a hello
```

仍是两个 transaction。crash 如果发生在两条 syscall 之间，可能留下“空文件已创建，但 hello 还没写”，这是正确事务边界，不是 journal failure。

---

## 2. fault model 明确限制

第一轮只在**显式 durable boundary**上给确定 old/new 结论。

课程把：

```text
某组 full-block WRITE completion
→ 随后的 FLUSH completion == OK
```

定义成可证明 durable checkpoint。

本阶段**不宣称**覆盖：

- 一个 4 KiB block 正在写到一半时真实突然掉电；
- sector/torn-write 任意撕裂；
- controller/drive 对 FLUSH 撒谎；
- 多设备 write ordering；
- 多核并发 transaction；
- 恶意篡改 journal。

journal/header/payload checksum 可以检测许多实验损坏，但“QEMU 在明确 FLUSH 边界后被终止”仍不等于真实硬件 power-loss 的完整模型。

> 仅有 WRITE request completion、但没有对应 FLUSH completion 时，教材不得预设 old 或 new；重启后只能读取实际 on-disk state 再解释。

---

## 3. 固定 journal 区域

Stage 7 format 已预留：

```text
block 5      journal header
block 6..20  journal payload slots
```

因此物理容量：

```text
15 payload blocks
```

一个 transaction 最多修改：

```text
MAX_TX_BLOCKS = 15 distinct home blocks
```

超过上限：

```text
TransactionTooLarge
```

必须在修改任何 persistent journal/home state 之前拒绝。

### 哪些 block 可以成为 home target

第一版允许：

```text
1,2,3,4
21..total_blocks-1
```

即：

```text
block/inode bitmap
inode table
data/directory blocks
```

明确禁止：

```text
0      superblock（本阶段不支持在线修改）
5..20  journal 自身
越过 filesystem/device capacity
重复 target block
```

这样 boot recovery 不需要先信任一个同时被当前 transaction 修改的 superblock。

---

## 4. journal header：block 5

整个 block 5 是一个 4096-byte header block。

version=1 非空 layout：

| offset | size | field |
| ---: | ---: | --- |
| 0 | 4 | magic，bytes=`JNL1` (`u32 LE 0x314c4e4a`) |
| 4 | 4 | version=`1` |
| 8 | 4 | state |
| 12 | 4 | txid |
| 16 | 4 | count，1..15 |
| 20 | 4 | header_checksum32 |
| 24 | 60 | home_block[15]，每项 u32 LE |
| 84 | 60 | payload_checksum[15]，每项 u32 LE |
| 144..4095 | — | reserved=0 |

state：

```text
1 = PREPARED
2 = COMMITTED
```

clean journal 不写 state=0 的普通 header，而是：

```text
整个 block 5 全 0
```

recovery 看到 full-zero header 即 EMPTY。

### checksum 教学算法

沿用 NEX1 的简单定义：

```text
checksum32(bytes) = 所有 byte 做 u32 wrapping sum
```

它用于发现实验中的损坏/错误 payload，不是密码学完整性或认证。

`header_checksum32`：

1. 暂时把 header checksum 字段 20..23 视为 0；
2. 对整个 4096-byte header 做 checksum32。

PREPARED→COMMITTED 改 state 后必须重新计算 header checksum。

每个 payload slot 也计算完整 4096 bytes checksum，并记录在对应 `payload_checksum[i]`。

未使用数组项和 reserved bytes 必须为 0。

---

## 5. 为什么需要独立 transaction staging

最重要的不变量：

> **在 durable COMMITTED 之前，任何本事务 home block 都不能被普通 cache writeback 写到 home location。**

否则：

```text
journal 还只是 PREPARED
但某个新 inode/bitmap/data 已经被 cache eviction 写到 home
```

recovery 再说“PREPARED 可以丢弃，恢复 old”就不成立。

### fixed transaction overlay

为一个 transaction 准备最多 15 个：

```text
TxBlock {
  home_block,
  data[4096],
}
```

可放在专用静态 `.bss` staging area：

```text
15 * 4096 = 61440 bytes
```

它由 kernel image/BSS 所有，frame allocator 初始化时自然属于 Reserved；不要挤占第 18 课小 heap arena。

### `txn_get(block)`

```text
if block 已在 staging:
    返回 staging copy
else:
    从 normal block cache 读取当前 home image
    copy 4096 bytes 到 free staging slot
    立即 release cache handle
    记录 home_block
    返回 staging copy
```

后续 filesystem mutation 对：

```text
bitmap/inode/directory/data
```

全部修改 staging image，不直接 mark normal home cache dirty。

同一 transaction 再读这个 block 必须先命中 staging，看到自己的新版本。

### 一次只允许一个 mutation transaction

当前 filesystem syscall 已串行、S-mode 不被 scheduler 在任意 Rust frame 中抢占，所以 Stage 8 明确：

```text
one active filesystem mutation transaction at a time
```

transaction 在 syscall 返回 user 之前完成 commit/home apply/clear。

read-only syscall 不会在中间观察半 transaction state。

---

## 6. Commit protocol：顺序不能交换

假设 staging 中有 N 个最终 home images。

### Phase 0：validate

在写 journal 前：

```text
1 <= N <= 15
targets unique
targets in allowed home range
payload/header checksum 全部生成成功
```

失败：

```text
丢弃 memory staging
journal/home 均不改变
```

### Phase 1：payload + PREPARED

```text
for i in 0..N:
    direct WRITE journal block (6+i) = staging[i].data

WRITE block 5 = PREPARED header
FLUSH
```

journal blocks 5..20 不属于 normal home cache，所以 journal I/O 使用独立受控 block-driver path。

只有 FLUSH success 后才有：

```text
DURABLE_PREPARED
```

在这个 checkpoint：

```text
home == old state
recovery 可以丢弃 PREPARED
```

payload slots 中的旧/新 bytes 没有合法 committed header 就没有事务语义。

### Phase 2：COMMITTED —— 唯一 commit point

```text
header.state = COMMITTED
重新计算 header_checksum
WRITE block 5
FLUSH
```

只有这次 FLUSH success 后：

```text
DURABLE_COMMITTED
```

从这一刻开始，即使 home 还完全没更新：

```text
transaction semantic == new
```

recovery 必须把全部 payload replay 到 targets。

**DURABLE_COMMITTED 是唯一 old/new 分界线。**

### Phase 3：apply home images

runtime 为保持 cache coherence：

```text
for each TxBlock:
    get normal home cache slot
    replace full 4096 bytes with staged image
    mark dirty
    release

sync transaction home blocks
FLUSH
```

成功后 home new state durable。

如果这里任何 WRITE/FLUSH 失败：

```text
绝不能 clear journal
journal 保持 durable COMMITTED
filesystem → recovery-required/read-only error mode
```

下次 boot 仍可 replay 全 transaction。

Stage 8 迁移完成后，所有 mutation 都必须走 transaction path；不能保留一个旧 helper 继续直接 mark home cache dirty。

### Phase 4：clear journal

只有 home apply FLUSH success 后：

```text
WRITE block 5 = 4096 bytes zero
FLUSH
```

payload blocks 6..20 不必清零；header EMPTY 后它们没有 active transaction 语义。

如果 clear WRITE/FLUSH 失败：

```text
报告 error
停止新 mutation
```

home 已 durable new；下次 boot 若仍看到 COMMITTED，只会幂等 replay 相同 images。

---

## 7. Recovery 必须早于 normal filesystem cache

boot 顺序：

```text
block driver init
→ direct read journal block 5
→ recover/clear journal
→ THEN initialize/use normal FS cache
→ mount filesystem
```

如果先 cache old home，再 direct replay：

```text
cache 仍可能保存 stale old copy
```

所以 recovery 完成前 normal cache 不对外工作。

---

## 8. Recovery state machine

### EMPTY

```text
header 全 0
→ 不 replay
→ mount
```

### MALFORMED

包括：

- bad magic/version/header checksum；
- count 0/>15；
- duplicate target；
- target 指向 0/5..20/越界；
- reserved bytes 非零；
- unknown state。

处理：

```text
RecoveryCorrupt
→ 不 clear
→ 不 RW mount
→ 保留 image 供 offline investigation
```

### PREPARED

协议保证 home 没有被本 transaction 修改：

```text
不 replay
→ zero header
→ FLUSH
→ old state
```

clear 失败则停止 RW mount。

### COMMITTED

先读取：

```text
blocks 6..6+count-1
```

并验证每个 full payload checksum。

任意 payload invalid：

```text
RecoveryCorrupt
→ 不 clear
→ 不猜测 replay
```

全部有效：

```text
for each payload:
    direct WRITE full 4096 bytes to target home block
FLUSH
WRITE zero header
FLUSH
```

然后 normal cache/mount 才开始。

---

## 9. 为什么 replay 是幂等的

redo payload 保存：

```text
完整最终 4096-byte home image
```

所以：

```text
write image X to block 42
write image X to block 42 again
```

结果相同。

如果 recovery 在 replay 第 3/8 个 home block 后 crash：

```text
COMMITTED header 仍在
```

下次 boot 从第 1 个 target 全量重写，最终仍是同样 new state。

这比记录：

```text
“bitmap +1”
“append entry”
```

之类非幂等 operation log 更容易证明。

---

## 10. Fault matrix

每个 case 从**同一个 clean baseline image 的新副本**开始。

### Transaction durable checkpoints

| 点 | durable evidence | recovery 后语义 |
| --- | --- | --- |
| T0 | transaction 开始前 | old |
| T1 | PREPARED FLUSH success | old |
| T2 | COMMITTED FLUSH success | new |
| T3 | home apply FLUSH success、header 仍 COMMITTED | new |
| T4 | zero-header FLUSH success | new，无需 replay |

不要对“WRITE completion 但没 FLUSH”的 kill 点写死 old/new 预期。

### Recovery replay 中间故障

正常 recovery 可以全部 home WRITE 后一次 FLUSH。

为了确定性测试幂等性，test mode 可以：

```text
replay home[0]
FLUSH
crash
```

下一 case：

```text
replay home[0..1]
FLUSH
crash
```

……

每个 test checkpoint 自己 FLUSH，所以“前 k 个 home durable new”是可证明的；下一 boot 必须再次看到 COMMITTED 并全量 replay N 个 target。

不要在 replay `for` 循环任意 instruction 上 kill 后假装知道哪些 writes durable。

---

## 11. 每个 crash case 的三个 oracle

### Oracle 1：journal state

成功 recovery 后：

```text
header 全 0
```

### Oracle 2：offline read-only fsck

第 45 课 checker：

```text
0 structural errors
```

### Oracle 3：semantic state

```text
before DURABLE_COMMITTED → old
at/after DURABLE_COMMITTED → new
```

例如 transaction：

```text
create /crash
```

则：

```text
old: /crash 不存在
new: /crash 存在且 inode/dir/bitmap 一致
```

file-write transaction 还要比较 exact old/new bytes。

`fsck clean` 不能单独替代 semantic oracle；old/new 两个状态都可能结构上完全 clean。

---

## 12. Stage 8 后的 runtime 规则

迁移完成后，所有 filesystem mutation：

```text
create
mkdir
write
allocation metadata change
```

必须使用 transaction staging + commit。

read-only operation 可以继续走 normal cache。

`sync()` 在每个 transaction 同步 commit/apply/clear 完成后通常没有 transaction-owned dirty home；它仍可保留为 global device/cache barrier 与回归接口。

每新增一种 mutation，都要重新回答：

```text
会修改哪些 distinct home blocks？
是否全部进入 staging？
是否 <= 15 blocks？
semantic oracle 是什么？
```

---

## 阶段最终能声称什么

可以说：

> 在 neonos 明确定义的 single-device、single-transaction、full-block、flush-boundary QEMU fault model 中，受支持的单个 filesystem mutation syscall 具有 old-or-new 原子恢复语义；合法 COMMITTED redo transaction 可重复 replay，并通过 journal、read-only fsck 与 semantic oracle 验证。

不能说：

> “已经证明真实硬盘任意断电绝对不会坏”。

进一步学习可研究：

- torn-write-resistant duplicated journal headers；
- checksummed / copy-on-write metadata；
- group commit；
- concurrent transactions；
- metadata journaling 的 ordered/writeback/data modes；
- per-file fsync dependency；
- device barriers/FUA；
- real hardware crash fault models。