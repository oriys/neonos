# 第八阶段：崩溃一致性与恢复

状态：课程已安排，学习待开始。入口：[总路线](README.md) · [进度记录](progress.md)。

目标：在第七阶段文件系统上稳定重现一次“写到一半”的不一致，建立离线一致性检查器，再实现一个范围明确、可验证、可重复恢复的 **full-block physical redo journal**。

阅读 OSTEP 第 42 章 Crash Consistency: FSCK and Journaling；第 43～45 章用于比较 LFS、SSD 与数据完整性，不要求第一轮全部实现。

> 第 44～47 课共同遵守 [第八阶段统一协议](stage-08-protocol.md)。如果逐课教材和统一协议出现冲突，以统一协议为准，并立即修正文档；课程不允许同时存在两套 commit/crash 语义。

## 开始前检查

必须先完成 [第七阶段](stage-07.md)：文件系统格式稳定、sync/flush 路径可验证、跨重启内容正确、坏格式会拒绝挂载。第七阶段已经把文件系统块 5～20 预留给日志，本阶段沿用这 16 个块，不改变既有数据区起点。

开始前固定一份已知正确的测试镜像并保存校验值。所有破坏性实验只针对副本；测试脚本默认拒绝把普通宿主机文件或设备节点当作实验盘。

## 9 次学习安排

每次 45～60 分钟，每周 2～3 次约 4～6 周。故障注入和恢复调试可以继续拆分。

| 次数 | 课程 | 当次任务 | 当次成果 |
| --- | --- | --- | --- |
| 1 | [44A 写入步骤与不变量](44-crash-consistency.md) | 拆解一次文件扩展会修改哪些持久块 | 写出事务前后都必须成立的不变量 |
| 2 | [44B 故障点实验](44-crash-consistency.md) | 在确定 flush 边界终止 QEMU | 稳定得到至少一种可解释损坏 |
| 3 | [45A 检查器扫描](45-fsck.md) | 解码超级块、位图、inode、目录 | 报告具体 inode/块/目录项位置 |
| 4 | [45B 一致性规则](45-fsck.md) | 检测重复块、悬空引用、位图矛盾 | 正常镜像零错误，坏镜像稳定报错 |
| 5 | [46A 日志格式与 staging](46-journal.md) | 定义 header、payload、事务 overlay | home 在 commit 前不会被普通 cache writeback 偷写 |
| 6 | [46B 提交协议](46-journal.md) | PREPARED → COMMITTED → apply → clear | durable COMMITTED 成为唯一 old/new 分界线 |
| 7 | [47A 启动恢复](47-recovery.md) | 重放 COMMITTED，丢弃 PREPARED | 恢复后 fsck 与语义 oracle 都通过 |
| 8 | [47B durable-boundary 故障矩阵](47-recovery.md) | 在可证明的持久化边界注入终止 | commit 前=old，commit 后=new |
| 9 | [47C replay 中断与幂等](47-recovery.md) | 恢复中再次崩溃、重复恢复、旧功能回归 | 同一 COMMITTED 可任意重复 replay |

## 第一轮统一日志模型

本阶段采用固定容量的**物理 redo log**。日志保存本次事务会修改的每一个 filesystem home block 的：

```text
home block number
+
完整 4096-byte 最终新镜像
```

包括本事务修改的：

- block/inode bitmap；
- inode-table block；
- directory block；
- regular-file data block。

因此第一轮**不是**“只 journal metadata，再靠 ordered data 保证内容”的模式。那是另一种设计，不能和本课程的 old-or-new 语义混写。

日志区域：

```text
block 5      header
block 6..20  15 个 payload slots
```

所以：

```text
MAX_TX_BLOCKS = 15 distinct home blocks
```

超过容量必须在产生任何持久修改前失败。

## 为什么必须有 transaction staging

Stage 7 的普通 cache 可以把 dirty victim 写回 home。若 Stage 8 先直接修改 home cache：

```text
journal 还没 durable COMMITTED
→ cache eviction 却先把新 inode/bitmap 写回 home
```

那么“commit 前恢复 old”立即失效。

因此 mutation syscall 先把需要修改的 block 复制到 transaction-owned staging：

```text
normal home image
→ copy 到 TxBlock staging
→ 所有修改只发生在 staging
```

在 COMMITTED durable 之前：

> 本事务的 home blocks 绝不能由普通 cache writeback 改写。

详细结构和 cache-coherence 规则见 [stage-08-protocol.md](stage-08-protocol.md)。

## 唯一 commit point

统一提交顺序：

```text
Phase 0  在内存 staging 准备所有最终 block image
  ↓
Phase 1  写 payload + PREPARED header
  ↓ FLUSH
DURABLE_PREPARED
  ↓
Phase 2  写 COMMITTED header
  ↓ FLUSH
DURABLE_COMMITTED   ← 唯一 commit point
  ↓
Phase 3  把完整 staged images 安装到 home blocks
  ↓ FLUSH
HOME_DURABLE
  ↓
Phase 4  把 header block 清为全 0
  ↓ FLUSH
EMPTY
```

恢复语义只有一条分界：

```text
没有 durable COMMITTED → old state
存在合法 durable COMMITTED → replay 后 new state
```

不能用“系统调用已经返回”“WRITE request completed”或“Rust 代码已经执行到某行”代替 commit。

## 故障模型：只对可证明的 durable state 下结论

第一轮 QEMU 实验只在**显式 flush 完成后的 durable checkpoint**给出确定 old/new oracle：

| checkpoint | 已知持久状态 | recovery 语义 |
| --- | --- | --- |
| T0 | 事务开始前 | old |
| T1 | PREPARED FLUSH 成功 | old |
| T2 | COMMITTED FLUSH 成功 | new |
| T3 | home apply FLUSH 成功，header 仍 COMMITTED | new |
| T4 | zero-header FLUSH 成功 | new，无需 replay |

对于：

```text
WRITE completion 之后
但对应 FLUSH 之前
```

课程**不写死 old/new 预期**。重启后先读取真实磁盘状态再诊断；不能假装自己知道 host cache 此刻到底持久了什么。

第一轮同样不宣称覆盖任意真实断电的 torn-sector/torn-4KiB write。header/payload checksum 用于检测损坏，但检测到 malformed journal 时进入只读诊断，不猜测恢复。

## Recovery 必须早于 normal filesystem cache

启动顺序：

```text
block driver
→ direct read/recover journal
→ journal 变 EMPTY
→ 才初始化/使用 normal FS cache
→ mount
```

否则先缓存 old home，再 direct replay，会留下 stale cache 副本。

## 三个恢复 oracle

每个 crash case 恢复后同时检查：

1. **journal oracle**：header 最终为 EMPTY；
2. **structural oracle**：只读 `fsck.py` 零结构错误；
3. **semantic oracle**：commit 前得到 old，commit 后得到 new；文件写事务还要比较 exact bytes。

只看到“mount 成功”不算通过。

## 计划产物

- [stage-08-protocol.md](stage-08-protocol.md)：统一 journal/crash 技术协议；
- `docs/fs-log-format.md`：实际 on-disk journal 字段；
- `tools/fsck.py`：独立只读离线检查器；
- `src/fs/journal.rs` 或等价模块：staging、commit、recovery；
- `tests/crash-recovery.sh`：baseline copy、fault injection、reboot、fsck、semantic oracle。

检查器和内核遵守同一格式约定，但检查器独立解码磁盘 bytes，不调用内核实现自证正确。

## 总验收

- [ ] 能解释 syscall success、WRITE completion、FLUSH completion、DURABLE COMMITTED 四个概念。
- [ ] 无日志版本可以稳定复现并解释至少一种元数据矛盾。
- [ ] fsck 对正常镜像零报错，对构造坏镜像指出具体对象和规则。
- [ ] 所有 mutation 在 commit 前只改 staging，不允许 home cache 偷写。
- [ ] PREPARED 恢复 old；合法 COMMITTED 必须 replay 到 new。
- [ ] 已提交事务在任意**可证明的 replay durable checkpoint**中断后仍可完整恢复。
- [ ] 恢复重复执行不改变最终 new state。
- [ ] 每个 crash case 同时通过 journal/fsck/semantic 三类 oracle。
- [ ] shell、文件 API、线程、fork/exec、旧 storage 测试全部回归。
- [ ] 能明确说出本阶段不覆盖 torn-write、真实设备撒谎、多设备和并发事务。

完成第八阶段后，第一轮课程闭环：在 neonos 明确定义的单设备、单事务、full-block、flush-boundary QEMU fault model 中，受支持的单个文件系统 mutation syscall 具有可验证的 old-or-new 恢复语义。