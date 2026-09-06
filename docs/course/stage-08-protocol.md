# 第八阶段统一协议：Full-block Redo Journal 与崩溃模型

这份文档是第 44～47 课共同遵守的**技术协议**。目的不是再增加一套文件系统功能，而是让四课对“什么叫 commit、什么能 replay、哪些 crash 结果可预测”使用同一个定义。

如果逐课教材和这里发生冲突，以这里的阶段协议为准，并在实现时同步回逐课文档。

---

## 1. 第一轮到底保证什么

Stage 8 对当前受支持的**单个 filesystem mutation syscall**提供：

```text
full-block physical redo transaction
```

journal 不只记录 metadata；它记录本次事务中**所有会被修改的 filesystem home blocks 的完整 4096-byte 新镜像**，包括 file data block。

因此在本阶段明确的 fault model 内：

```text
crash before durable COMMITTED
→ recovery 得到事务前 old state

crash after durable COMMITTED
→ recovery 最终得到事务后 new state
```

不会出现“metadata 是新的但 data 只保证 ordered”这种半强语义。

### 原子性范围是一个 syscall/事务

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

仍是两个事务。crash 发生在两条 syscall 之间时，可能留下“空文件已经创建，但 hello 还没写”，这是正确的事务边界，不是 journal 失败。

---

## 2. fault model 明确限制

第一轮只在**显式 durable boundary 之间**注入 crash。

我们可以确定：

```text
某组 full-block WRITE completion
→ FLUSH completion OK
→ 才定义为 durable checkpoint
```

然后测试 harness 强制结束 QEMU。

本阶段**不宣称**覆盖：

- 一个 4 KiB block 正在写到一半时突然真实掉电；
- sector/torn-write 任意撕裂；
- controller/drive 对 flush 撒谎；
- 多设备 write ordering；
- 多核并发 transaction；
- 恶意篡改 journal。

journal/checksum 可以检测不少损坏，但“QEMU 在明确 flush 边界后退出”仍然不是现实硬件 power-loss 的完整模型。

---

## 3. 固定 journal 区域

Stage 7 format 已经预留：

```text
block 5      journal header
block 6..20  journal payload slots
```

因此物理容量：

```text
15 payload blocks
```

一个事务最多修改：

```text
MAX_TX_BLOCKS = 15 distinct home blocks
```

超过上限：

```text
TransactionTooLarge
```

必须在修改任何 home state 之前拒绝。

当前 create/mkdir/<=256-byte write 预期远小于 15，但测试仍必须覆盖容量边界。

### 哪些 block 可以成为 home target

第一版允许：

```text
1,2,3,4
21..total_blocks-1
```

即 bitmap/inode table/data+directory blocks。

明确禁止：

```text
0      superblock（本阶段不支持在线修改）
5..20  journal 自身
越过 device/fs capacity
重复 target block
```

这样 boot recovery 不需要先信任一个可能被事务同时修改的 superblock。

---

## 4. journal header：block 5

整个 block 5 是一个 4096-byte header block。

非空 header 的 version=1 layout：

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

clean journal 不写一个 state=0 header，而是：

```text
整个 block 5 全 0
```

recovery 看到全零即 EMPTY。

### checksum 教学算法

沿用 NEX1 的简单定义：

```text
checksum32(bytes) = 所有 byte 做 u32 wrapping sum
```

它用于发现实验中的截断/错误 payload，不是密码学完整性。

`header_checksum32`：

1. 暂时把 header checksum 字段 20..23 当作 0；
2. 对整个 4096-byte header 做 checksum32。

PREPARED→COMMITTED 改 state 后必须重新计算 header checksum。

每个 payload slot 也计算完整 4096 bytes 的 checksum，写在对应 `payload_checksum[i]`。

未使用数组项和 reserved bytes 必须为 0。

---

## 5. 为什么 transaction 需要独立 staging，不直接先改 home cache

最重要的不变量：

> **在 durable COMMITTED 之前，任何本事务 home block 都不能被普通 cache writeback 写到 home location。**

否则：

```text
journal 还只是 PREPARED
但某个新 inode/bitmap 已经被 cache eviction 写到 home
```

recovery 再说“PREPARED 可以丢弃，恢复 old state”就不成立。

### 使用 fixed transaction overlay

为一个事务准备最多 15 个：

```text
TxBlock {
  home_block,
  data[4096]
}
```

可以放在专用静态 `.bss` staging area：

```text
15 * 4096 = 61440 bytes
```

这块内存由 kernel image/BSS 所有，frame allocator 初始化时自然属于 Reserved；不要挤占第 18 课小 heap arena。

### `txn_get(block)`

```text
if block 已在 staging:
    返回 staging copy
else:
    从 normal block cache 读取当前 home image
    copy 4096 bytes 到一个 free staging slot
    立即 release cache handle
    记录 home_block
    返回 staging copy
```

后续 filesystem mutation：

```text
bitmap/inode/directory/data
```

全部改 staging image，而不是直接 mark normal home cache dirty。

同一事务再次读这个 block 必须看到 staging 新版本。

### 一次只允许一个 mutation transaction

当前 filesystem syscall 已串行、S-mode 不被 scheduler 抢占，所以 Stage 8 明确：

```text
one active filesystem transaction at a time
```

事务在 syscall 返回用户之前完成 commit/home apply/clear。

读-only syscall 不会在中间观察半事务状态。

---

## 6. Commit protocol：顺序不能交换

假设 staging 中有 N 个最终 home images。

### Phase 0：validate

在写 journal 之前：

```text
1 <= N <= 15
targets unique
targets in allowed home range
payload/header checksum 全部计算完成
```

失败 → 丢弃内存 staging，home/journal 均不改变。

---

### Phase 1：写 payload + PREPARED

```text
for i in 0..N:
    direct WRITE journal block (6+i) = staging[i].data

WRITE block 5 = PREPARED header
FLUSH
```

journal blocks 5..20 不属于 normal home cache，所以 journal I/O 可以走独立受控 block-driver path，不和某个 home cache key 产生双副本。

只有 FLUSH success 后才有：

```text
DURABLE_PREPARED
```

如果 crash 发生在这个 checkpoint：

```text
home 仍是 old state
recovery 可以丢弃 PREPARED
```

payload slot 中旧/新垃圾没有 header commit 就没有事务语义。

---

### Phase 2：写 COMMITTED header —— 唯一 commit point

```text
把 header.state 改成 COMMITTED
重新计算 header_checksum
WRITE block 5
FLUSH
```

只有这次 FLUSH success 后：

```text
transaction is committed
```

从这一刻开始，即使 home 还完全没更新，recovery 也必须把 payload replay 到所有 targets。

**COMMITTED durable 是唯一 old/new 分界线。**

---

### Phase 3：apply home images

为了保持 block cache coherence：

```text
for each TxBlock:
    get normal home cache slot
    replace full 4096 bytes with staged image
    mark dirty
    release

filesystem sync dirty home blocks
FLUSH
```

成功后 home new state durable。

如果这里任何 write/flush 失败：

```text
绝不能 clear journal
journal 继续保持 durable COMMITTED
filesystem 进入 recovery-required/read-only error mode
```

下次 boot 仍能 replay 全部 transaction。

当前 Stage 8 所有 mutation 都先 staging，因此不会混入“本事务之外的旧 dirty mutation”。进入事务前可断言 normal FS cache 没有未归属事务的 dirty home block；迁移完成后所有修改 syscall 都必须走 transaction path。

---

### Phase 4：clear journal

只有 home apply FLUSH 成功后：

```text
WRITE block 5 = 4096 bytes zero
FLUSH
```

payload blocks 6..20 不必清零；header 全零后它们没有 active transaction 语义。

如果 clear WRITE/FLUSH 失败：

```text
报告 error
停止新 mutation
```

但 home 已经是 durable new state；如果下次 boot 仍看到 COMMITTED header，只会幂等 replay 同样的新 image。

---

## 7. Recovery 必须在 normal mount/cache 之前

boot 顺序：

```text
block driver init
→ read journal block 5 directly
→ recover/clear journal
→ THEN initialize/use normal filesystem cache
→ mount filesystem
```

原因：如果先把 old home block 缓存起来，再 direct replay journal：

```text
cache 仍可能保存 stale old copy
```

recovery 完成前不允许 normal home cache 对外工作。

---

## 8. Recovery state machine

### Header 全 0

```text
EMPTY
→ 不 replay
→ 正常 mount
```

### 非零但 header 无效

包括：

- bad magic/version；
- bad checksum；
- count 0/>15；
- duplicate target；
- target 指向 0/5..20/越界；
- reserved bytes 非零；
- unknown state。

处理：

```text
RecoveryCorrupt
→ 不 clear
→ 不 mount read-write
→ 保留 image 供 fsck/离线调查
```

不要“猜它大概没事”。

### PREPARED

协议保证：

```text
home 尚未在事务中被修改
```

所以：

```text
不 replay payload
→ clear header zero
→ FLUSH
→ mount old state
```

clear 失败则停止 read-write mount。

### COMMITTED

先读取：

```text
blocks 6..6+count-1
```

验证每个 4096-byte payload checksum。

任何 payload invalid：

```text
RecoveryCorrupt
→ 不清 header
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

然后才 normal mount/cache。

---

## 9. 为什么 replay 是幂等的

redo payload 保存的是：

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

下次 boot 从第 1 个开始全部再写一遍，最终仍得到同样 new state。

这比保存“把 bitmap bit +1 / append 一个 entry”之类 operation log 更容易保证幂等。

---

## 10. Stage 8 fault matrix

每个 case 都从**同一个干净 baseline image 的新副本**开始，不能连续在一个已经被上个 crash 改过的 image 上跑。

### Transaction fault points

| 点 | durable evidence | recovery 后语义 |
| --- | --- | --- |
| T0 | transaction 开始前 | old |
| T1 | PREPARED FLUSH 成功 | old |
| T2 | COMMITTED FLUSH 成功 | new |
| T3 | home apply FLUSH 成功、header 尚 COMMITTED | new |
| T4 | zero-header FLUSH 成功 | new，不需要 replay |

不要对“WRITE completion 但没 FLUSH”的任意 kill 点写死 old/new 预期。

### Recovery replay 中间故障

正常 recovery 可以所有 home writes 后只 flush 一次。

为了**确定性测试 replay 幂等性**，专用 test mode 可以：

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

因为每个 test checkpoint 自己显式 flush，所以“前 k 个 home 已 durable”是可证明的。

每次下次 boot 都必须：

```text
重新看到同一个 COMMITTED
→ replay 全部 N
→ 最终 new state
```

不要用“在 for 循环任意 instruction 上 kill QEMU”来假装知道哪些 writes 已 durable。

---

## 11. 每个 crash case 的三个 oracle

恢复测试不是只看“mount 成功”。

### Oracle 1：journal state

```text
recovery 后 header 全 0
```

### Oracle 2：offline read-only fsck

第 45 课 checker 必须：

```text
0 structural errors
```

### Oracle 3：semantic state

根据 commit point：

```text
before COMMITTED → old state
at/after COMMITTED → new state
```

例如事务是：

```text
create /crash
```

则 old/new 不是“文件系统不坏”这么宽泛，而是：

```text
old: /crash 不存在
new: /crash 存在且 inode/dir/bitmap 全一致
```

full-data write transaction 还要比较 exact file bytes。

---

## 12. Stage 8 后的 runtime 规则

迁移到 journal 后，所有 filesystem mutation：

```text
create
mkdir
write
allocation metadata change
```

必须使用 transaction staging + commit。

不能留一条老路径：

```text
某个 inode write 仍然直接 mark home cache dirty
```

否则 PREPARED→old 的核心 invariant 被破坏。

read-only operation 可以继续走 normal cache。

`sync()` 在每个事务同步 commit/apply/clear 都完成后通常没有 transaction dirty home；它仍可保留为全局 device/cache barrier 与回归检查接口。

---

## 阶段最终能声称什么

可以说：

> 在 neonos 明确定义的单设备、单事务、full-block、flush-boundary QEMU fault model 中，受支持的单个文件系统修改 syscall 具有 old-or-new 原子恢复语义；COMMITTED redo transaction 可重复 replay，并通过 read-only fsck 与语义 oracle 验证。

不能说：

> “已经证明真实硬盘任意断电绝对不会坏”。

下一步如果继续进阶，再研究：

- torn-write-resistant duplicated journal headers；
- checksummed/copy-on-write metadata；
- group commit；
- concurrent transactions；
- data journaling vs ordered/writeback modes；
- fsync dependency；
- device barriers/FUA；
- real crash hardware fault models。
