# 第 47 课：启动时恢复，并验证每个 durable 边界

状态：待开始。前置：[第 46 课](46-journal.md)。分 A、B、C 三次，每次 45～60 分钟。

## 目标与统一协议

本课不再设计新的 journal 语义，而是实现并验证 [第八阶段统一协议](stage-08-protocol.md)：

```text
没有 durable COMMITTED → old state
存在合法 durable COMMITTED → replay 后 new state
```

恢复必须发生在 normal filesystem cache 和 mount 之前；测试只在**能够证明已经 durable 的 FLUSH checkpoint**上给出确定 old/new oracle。

---

# A 次：启动恢复状态机

## 1. recovery 为什么必须早于 normal cache

启动顺序固定为：

```text
block driver init
→ direct read journal block 5
→ recover / clear journal
→ journal 成为 EMPTY
→ 才初始化 normal filesystem cache
→ mount filesystem
```

如果反过来：

```text
先把 old home block 放进 cache
→ 再 direct replay journal 到 device
```

那么 cache 仍可能保留 stale old copy，之后一次 hit/dirty eviction 又把恢复结果弄乱。

所以 recovery 期间：

> normal home cache 不对外工作；journal/home replay 使用受控 direct block-I/O 路径。

---

## 2. 第一件事不是 replay，而是验证 header

读取 block 5。

### 整块全 0

```text
EMPTY
→ 不 replay
→ 进入正常 mount
```

### 非零 header

按 `fs-log-format.md` / 统一协议逐项验证：

- magic/version；
- header checksum；
- state 只能 PREPARED/COMMITTED；
- `1 <= count <= 15`；
- home targets 唯一；
- target 在允许 filesystem home 范围；
- target 不能是 superblock 0；
- target 不能是 journal 5..20；
- target 不越过 filesystem/device；
- 未使用数组项和 reserved bytes 必须为 0。

任何一项失败：

```text
RecoveryCorrupt
→ 不 replay
→ 不 clear
→ 不 read-write mount
→ 保留镜像供 fsck / dump-journal 调查
```

不要因为“看起来像上一次事务”就猜着修。

---

## 3. PREPARED：恢复 old

统一协议保证：

> 在 durable COMMITTED 之前，本事务的 home blocks 没有被 normal cache/writeback 改写。

因此合法 PREPARED 表示：

```text
journal payload 已 durable
home 仍是 old
transaction 尚未 commit
```

恢复：

```text
不 replay payload
→ WRITE block 5 = 4096 bytes zero
→ FLUSH
→ EMPTY
→ mount old state
```

clear WRITE/FLUSH 失败：

```text
停止 read-write mount
```

不能为了继续启动而假装日志已经清理。

---

## 4. COMMITTED：先验证全部 payload，再 replay

读取：

```text
block 6 .. 6+count-1
```

对每个 payload：

- 必须完整读取 4096 bytes；
- checksum 与 header 中记录一致。

只要一个 payload 不合法：

```text
RecoveryCorrupt
→ 一个 home block 都不要猜测性修复
→ header 保留
→ 不 read-write mount
```

全部合法后：

```text
for each payload:
    direct WRITE full 4096 bytes to its home target

FLUSH
```

只有 home replay FLUSH 成功后，才允许：

```text
WRITE zero header
FLUSH
```

随后 journal EMPTY，才初始化 normal cache/mount。

### 为什么无论 home 已安装多少都“全量 replay”

crash 时可能已经：

```text
0 个 home 是 new
部分 home 是 new
全部 home 是 new
```

但 redo payload 保存的是每个目标块的**完整最终 4096-byte image**。

所以恢复不猜“哪些已经做过”，而是：

```text
从第一个 target 开始全部写一遍
```

这正是幂等性的来源。

### A 次验收

- [ ] recovery 在 normal cache/mount 前执行。
- [ ] malformed journal 不 replay、不 clear、不 RW mount。
- [ ] PREPARED clear 后得到 old。
- [ ] COMMITTED 先验证全部 payload，再完整 replay。
- [ ] replay FLUSH 成功以后才 clear header。
- [ ] clear 失败不会被当成“恢复完成”。

---

# B 次：只在 durable checkpoint 上做 old/new 故障矩阵

计划增加：

```text
tests/crash-recovery.sh
```

每个案例都：

```text
从同一份已知正确 baseline image
→ 复制一个新的工作副本
→ 运行一个确定 mutation
→ 到指定 durable checkpoint 后终止 QEMU
→ 用同一个工作副本重启恢复
```

不能连续复用上一个 crash case 的坏/已恢复镜像。

## Transaction checkpoint

| 点 | 终止前已经得到的 durable evidence | recovery 后语义 |
| --- | --- | --- |
| T0 | transaction 开始前 | old |
| T1 | payload + PREPARED header 的 FLUSH 已成功 | old |
| T2 | COMMITTED header 的 FLUSH 已成功 | new |
| T3 | 所有 home apply 的 FLUSH 已成功，header 仍 COMMITTED | new |
| T4 | zero-header 的 FLUSH 已成功 | new，启动时无需 replay |

这五个点足够验证真正的协议分界：

```text
T1 → old
T2 → new
```

### 明确禁止一个常见“伪确定性”

如果只知道：

```text
COMMITTED header WRITE request completed
```

但：

```text
commit FLUSH 还没完成
```

课程不能预设 commit 一定持久，也不能预设一定没持久。

同理：

```text
zero-header WRITE completed
但 final clear FLUSH 未完成
```

也不能写死“journal 一定 empty”。

这些点可以做**观察实验**：

```text
kill
→ reboot 前由 dump-journal 读取实际磁盘状态
→ 再按真实 EMPTY/PREPARED/COMMITTED/MALFORMED 状态解释
```

但它们不是 old/new 的确定性验收点。

---

## 每个 case 必须有三个 oracle

### Oracle 1：journal

成功恢复后：

```text
block 5 == 全 0
```

### Oracle 2：structural fsck

运行第 45 课只读 checker：

```text
0 structural errors
```

### Oracle 3：semantic state

根据 commit point 检查真正业务状态。

例如事务：

```text
create /crash
```

则：

```text
old → /crash 不存在
new → /crash 存在，inode/dir/bitmap 全一致
```

如果事务是 file write：

```text
old → exact old bytes
new → exact new bytes
```

不能只看：

```text
“mount 成功”
```

因为一个结构上可挂载的旧文件内容仍可能违反本事务的 commit 语义。

### B 次验收

- [ ] T0/T1 恢复 old。
- [ ] T2/T3/T4 最终得到 new。
- [ ] 未 FLUSH 的 WRITE completion 不被强行赋予 durable 结论。
- [ ] 每个 case 都从 baseline 新副本开始。
- [ ] 每个 case 同时通过 journal/fsck/semantic 三个 oracle。

---

# C 次：故意在 recovery 中再次崩溃

正常 recovery 可以：

```text
写完全部 N 个 home
→ 一次 FLUSH
```

但如果我们在 replay loop 的任意 source line kill QEMU，无法可靠知道前面哪些 WRITE 已 durable。

所以为了**确定性验证幂等**，增加仅测试使用的 recovery checkpoint 模式。

## replay 前 k 个 block 后显式 FLUSH

假设 committed transaction 有 N 个 target。

case 1：

```text
replay home[0]
→ FLUSH
→ RECOVERY_FP_1
→ harness kill QEMU
```

case 2：

```text
replay home[0..1]
→ FLUSH
→ kill
```

……直到：

```text
replay home[0..N-1]
→ FLUSH
→ 但还没 clear header
→ kill
```

因为每个测试点自己 FLUSH，所以我们能证明：

```text
前 k 个 home 已 durable new
其余 home 可能仍 old
COMMITTED header 仍 durable
```

下一次启动必须：

```text
再次看到同一个 COMMITTED
→ 从 home[0] 开始全量 replay N 个 block
→ FLUSH
→ clear + FLUSH
→ 得到完整 new
```

## 为什么“重复写相同 full block image”是幂等的

```text
block 42 := image X
block 42 := image X
```

最终仍是：

```text
image X
```

journal 记录的不是：

```text
“把某个计数 +1”
“再 append 一个 entry”
```

这种重复执行会产生额外副作用的 operation log。

## 再测试 clear 前崩溃

恢复：

```text
全部 home replay
→ FLUSH 成功
→ header 仍 COMMITTED
→ crash
```

下一次启动仍然 replay 一遍，结果不应改变。

如果第二次恢复后：

- inode 数增加；
- bitmap 多占一块；
- 目录多一个重复 entry；

说明实现其实在 replay“操作”，而不是覆盖完整最终 block image。

---

## 多轮事务与资源回归

连续执行若干受支持 mutation，例如：

```text
create
mkdir
small file write
```

每一轮正常完成后验证：

```text
journal EMPTY
fsck clean
semantic result correct
journal blocks 5..20 从未被 normal allocator 返回
```

再回归：

```text
shell
fork/exec/wait
thread/sync
file API
磁盘 NEX1 program
旧 storage tests
```

确保 journaling 没有把 Stage 1～7 的行为破坏。

---

## 当前保证与不保证

### 可以声称

在 neonos 明确定义的：

```text
single device
single active FS mutation transaction
full 4096-byte redo images
flush-boundary QEMU fault model
```

中，受支持的单个 mutation syscall 具有：

```text
commit 前 → old
commit 后 → new
```

的可恢复原子语义；合法 COMMITTED transaction 可以任意重复 replay。

### 不能声称

本课程没有证明：

- 真实设备任意突然断电都不会 torn write；
- 4 KiB block 写入天然原子；
- controller/drive 一定诚实实现 FLUSH；
- 多设备写入原子；
- 多核并发 transaction；
- 恶意修改 journal 可安全恢复；
- 所有未来 filesystem 操作自动继承 crash atomicity。

每新增一种 mutation，都必须明确它是否完整进入 transaction staging，以及是否超过日志容量。

---

## 常见问题

| 现象 | 先检查 |
| --- | --- |
| T1 恢复成 new | commit 前是否有 home cache 偷写，是否违反 staging invariant |
| T2 后恢复成 old | COMMITTED header/payload 是否真正 FLUSH durable，recovery 是否误丢 commit |
| recovery 一次成功、第二次多分配资源 | 是否 replay operation 而不是 full-block image |
| fsck 通过但 file bytes 不符合 commit | 是否只检查结构 oracle，漏掉 semantic oracle |
| 未 flush 点结果偶尔变化 | 该点本来就没有 deterministic durable 结论 |
| recovery direct-write 后 mount 读旧值 | 是否在 normal cache 初始化之后才 replay，留下 stale cache |

## 第一轮毕业验收

- [ ] 从 QEMU/OpenSBI 讲清内核启动入口。
- [ ] 用户程序能通过系统调用、调度和地址空间隔离运行。
- [ ] 能解释线程、锁、条件变量、信号量和 deadlock。
- [ ] shell 能运行内嵌及磁盘程序，文件可跨正常重启保存。
- [ ] 无日志版本能稳定重现一种 crash inconsistency。
- [ ] fsck 能独立发现并定位该不一致。
- [ ] Stage 8 只有一份统一 full-block redo 协议。
- [ ] PREPARED/COMMITTED 分别给出 old/new 的确定恢复语义。
- [ ] 所有确定性 crash case 都位于可证明的 durable checkpoint。
- [ ] recovery 中断后再次恢复仍得到 exact new state。
- [ ] 恢复后 journal EMPTY、fsck clean、semantic oracle 正确。
- [ ] 能明确说出教学 QEMU fault model 没有保证什么。

完成后，第一轮 47 个学习单元闭环。进阶可继续研究 duplicated/torn-write-resistant journal metadata、COW、真实换页、多核以及更完整的 filesystem crash model。