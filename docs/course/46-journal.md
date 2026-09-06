# 第 46 课：先把最终块镜像写进日志，再修改 home

状态：待开始。前置：[第 45 课](45-fsck.md)。分 A、B 两次，每次 45～60 分钟。

## 目标与统一协议

继续阅读 OSTEP 第 42 章 journaling。本课实现一个固定容量的教学 **full-block physical redo log**。

第 44～47 课共同遵守 [第八阶段统一协议](stage-08-protocol.md)。本课不能另起一套“metadata-only / ordered-data”语义。

最终只记住一条：

> **DURABLE COMMITTED 是唯一 commit point；在它之前 home 必须保持 old，在它之后 recovery 必须能得到完整 new。**

---

# A 次：先定义日志格式和 transaction staging

## 1. 固定 journal 容量

第七阶段已经预留：

```text
block 5      journal header
block 6..20  payload slots
```

因此：

```text
MAX_TX_BLOCKS = 15
```

一个 payload 对应：

```text
1 个 home block number
+
完整 4096-byte 最终新镜像
```

事务修改超过 15 个 distinct home blocks：

```text
TransactionTooLarge
```

必须在写任何 journal/home 持久块之前失败。

## 2. `docs/fs-log-format.md` 必须逐字段定义

按照 [统一协议](stage-08-protocol.md) 写出 block 5 的 version=1 header：

- magic/version；
- `state = PREPARED / COMMITTED`；
- `txid`；
- `count`；
- `home_block[15]`；
- `payload_checksum[15]`；
- `header_checksum32`；
- reserved bytes 全 0。

clean journal：

```text
block 5 全 0
```

不是“state=0 的普通 header”。

宿主机只读 decoder 至少区分：

```text
EMPTY
PREPARED
COMMITTED
MALFORMED
```

任何 unknown state、重复 target、越界 target、bad checksum、reserved 非零都不能被猜成有效事务。

## 3. 为什么不能先修改 normal home cache

Stage 7 的 cache 允许 dirty victim 被 writeback。

错误实现：

```text
先修改 inode cache / bitmap cache
→ journal 还没 COMMITTED
→ cache eviction 把新 metadata 写到 home
→ crash
→ recovery 看到 PREPARED，想恢复 old
→ 但 home 已经部分 new
```

于是 redo 协议失效。

所以 Stage 8 新增 transaction-owned staging：

```text
TxBlock {
    home_block,
    data[4096],
}
```

最多 15 个。

`txn_get(block)`：

```text
如果已经 staging → 返回同一份 staged image
否则：
    从 normal cache 读取当前 home image
    copy 4096 bytes 到 staging
    立即 release normal cache handle
    记录 home block
```

之后本次 mutation 对：

```text
bitmap
inode table
directory block
regular-file data block
```

的修改**全部写 staging**，不直接 mark home cache dirty。

### 第一轮一次只允许一个 mutation transaction

当前仍是：

```text
single hart
filesystem mutation syscall 串行
S-mode 不在任意 Rust 栈上调度
```

所以第一版只允许：

```text
one active FS mutation transaction
```

不做 group commit / concurrent transaction。

## 4. full-block redo 包含 data

这是本次质量 review 后必须统一的地方。

第一轮日志事务覆盖本 syscall 修改的**全部 home blocks**，包括 file data block。

因此：

```text
write(fd, <=256 bytes)
```

如果修改：

- data block 42；
- inode-table block 3；

那么这两个最终 4 KiB image 都进入同一个 redo transaction。

课程不再写：

```text
“data 先 ordered flush，journal 只保护 metadata”
```

因为那是另一种 journaling 模式，会改变 crash 保证和测试 oracle。

## A 次验收

- [ ] `fs-log-format.md` 与统一协议字段完全一致。
- [ ] decoder 能区分 EMPTY/PREPARED/COMMITTED/MALFORMED。
- [ ] `MAX_TX_BLOCKS=15`，重复/非法 target 被拒绝。
- [ ] mutation 的所有修改先进入 TxBlock staging。
- [ ] COMMITTED 之前 normal home cache 没有本事务 dirty home block。
- [ ] regular-file data block 和 metadata 一样进入 full-block redo transaction。

---

# B 次：实现唯一正确的提交顺序

假设 staging 有 N 个最终 block images。

## Phase 0：validate

在任何 persistent write 前验证：

```text
1 <= N <= 15
home targets 唯一
home target 在允许范围
不能指向 superblock 0
不能指向 journal 5..20
不能越过 filesystem/device
所有 payload/header checksum 可生成
```

失败：

```text
丢弃 staging
home/journal 不改变
```

---

## Phase 1：写 payload + PREPARED

```text
WRITE block 6+i = TxBlock[i].data
...
WRITE block 5 = PREPARED header
FLUSH
```

只有这次 FLUSH 成功后，才能说：

```text
DURABLE_PREPARED
```

此时：

```text
home 仍全部 old
```

所以 crash 后 PREPARED 可以丢弃，恢复 old state。

journal blocks 5..20 不走 normal filesystem home cache，避免和普通 home cache 形成双副本。

---

## Phase 2：写 COMMITTED —— 唯一 commit point

```text
header.state = COMMITTED
重新计算 header checksum
WRITE block 5
FLUSH
```

只有 FLUSH success 才有：

```text
DURABLE_COMMITTED
```

从这一刻开始事务语义已经是：

```text
NEW
```

即使所有 home blocks 现在仍是 old，recovery 也必须靠完整 payload replay 成 new。

不要把：

```text
COMMITTED WRITE request completed
```

误写成 durable commit；没有对应 FLUSH，就没有确定 durable checkpoint。

---

## Phase 3：apply home images

为了和 block cache 保持一致：

```text
for each TxBlock:
    get normal home cache slot
    replace full 4096 bytes with staged image
    mark dirty
    release

sync transaction home blocks
FLUSH
```

如果任意 home WRITE/FLUSH 失败：

```text
journal 必须继续保持 COMMITTED
绝不 clear
filesystem 进入 recovery-required/read-only error mode
```

下一次 boot 重新 replay 全部 payload。

### 为什么不能 direct-write home 后保留 stale cache

如果 recovery/commit 直接改设备 home，而 normal cache 仍保留旧 block：

```text
下一次 cache hit 可能读 old
未来 dirty eviction 甚至可能把 old 写回去
```

所以 runtime apply 使用 coherent cache path；boot recovery 则在 **normal cache 初始化之前** direct replay。

---

## Phase 4：clear journal

只有 home apply FLUSH 成功后：

```text
WRITE block 5 = 4096 bytes zero
FLUSH
```

payload blocks 6..20 不需要清零；header EMPTY 后它们没有 active transaction 语义。

如果 clear write/flush 失败：

```text
报告错误
停止新 mutation
```

home 已 durable new；如果下次启动仍看到合法 COMMITTED，只会幂等 replay相同 full-block images。

---

## failure policy

### payload/PREPARED 阶段失败

没有 durable COMMITTED：

```text
不安装 home
停止当前 mutation
进入明确 I/O error/read-only policy
```

不要假装设备失败后一定能“写回旧磁盘”。

### COMMITTED FLUSH 结果未知/失败

不能继续安装 home 并向用户宣称成功。

保留 journal 原样，停止新 mutation；重启后只根据**实际读到且校验通过的 journal state**决定恢复。

### home apply 失败

COMMITTED 保留，交给 recovery。

### clear 失败

COMMITTED/可能未清状态保留；重复 replay 必须安全。

---

## fault-point 原则

本课确定性验收只使用：

```text
T1 PREPARED FLUSH 成功
T2 COMMITTED FLUSH 成功
T3 home apply FLUSH 成功
T4 zero-header FLUSH 成功
```

不要在：

```text
“某个 WRITE completion 后但 FLUSH 前”
```

直接写死 old/new 结论。

那类点可以用于观察，但只能重启后读取实际 journal/home 状态再解释。

## 验收

- [ ] PREPARED FLUSH 后 crash → old state。
- [ ] COMMITTED FLUSH 后、home apply 前 crash → recovery 必须得到 new state。
- [ ] home apply 失败/中断不会清除 COMMITTED。
- [ ] clear 只发生在 durable home apply 之后。
- [ ] transaction staging 阻止 commit 前 home cache 偷写。
- [ ] data+metadata 都属于同一个 full-block redo transaction。
- [ ] 能解释为什么 COMMITTED FLUSH 是唯一 old/new 分界线。

下一课：[第 47 课：启动时恢复，并验证 durable 边界和 replay 幂等](47-recovery.md)。