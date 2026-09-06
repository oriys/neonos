# 第七阶段：块设备与文件系统

状态：课程已安排，学习待开始。入口：[总路线](README.md) · [进度记录](progress.md)。

目标：在**不会误写宿主机真实磁盘**的前提下，从一个专用 QEMU raw image 开始，完成 modern VirtIO block READ/WRITE/FLUSH、固定 block cache、精确定义的 on-disk format、inode/directory 和 user file API，最终从磁盘运行一个 NEX1 用户程序。

阅读 OSTEP 第 36～41 章；设备实现对照 [VirtIO 1.2](https://docs.oasis-open.org/virtio/virtio/v1.2/virtio-v1.2.html) 与当前 QEMU `virt` 平台资料。

## 开始前边界

保留前六阶段：

- single hart；
- S-mode kernel syscall path 不被 scheduler 任意抢占；
- user Threads/pipe/shell 已稳定；
- block/filesystem 第一版同步串行；
- 不实现 PLIC block interrupt，先 polling used ring；
- 不实现 async I/O；
- 不实现 deletion/rename/link/symlink；
- 不实现 crash journal，blocks 5..20 只预留给 Stage 8。

设备等待会带来有界 kernel latency；这不是高性能存储栈，而是为了先把**协议、所有权、持久化层次**做对。

## 17 次学习安排

| 次数 | 课程 | 当次问题 | 当次成果 |
| --- | --- | --- | --- |
| 1 | [36 实验磁盘](36-disk.md) | 怎样绝不碰宿主机真实磁盘 | 16 MiB protected raw image、sector/block 几何、DTB 记录 |
| 2 | [37A VirtIO 初始化](37-block-read.md) | modern device 怎样正确进入 DRIVER_OK | feature/status/queue 状态机、physical queue addresses |
| 3 | [37B 第一条 READ](37-block-read.md) | descriptor/avail/used 怎样交接 ownership | sector 64 exact marker、方向/fence/status/used-id 正确 |
| 4 | [37C 复用/timeout](37-block-read.md) | index wrap 和慢设备怎样安全处理 | 100 次 read 无 descriptor leak；timeout reset 后才复用 |
| 5 | [38A WRITE/readback](38-block-write.md) | WRITE completion 能证明什么 | 两 sector 独立写 + 不同 buffer readback |
| 6 | [38B FLUSH/reboot](38-block-write.md) | 怎样证明跨 QEMU 生命周期仍存在 | 真 FLUSH、记录 cache mode、same-image Boot2 readback |
| 7 | [39A cache](39-cache.md) | 重复 block 如何避免重复 I/O | 8 slots、key/pin、deterministic replacement |
| 8 | [39B dirty/sync](39-cache.md) | clean 和 durable 为什么不同 | dirty writeback、`needs_flush`、失败重试 |
| 9 | [40A on-disk format](40-format.md) | host/kernel 怎样解释同一 bytes | exact offsets/LE/bitmap/inode/dir/superblock 文档 + mkfs/dumpfs |
| 10 | [40B mount/allocator](40-format.md) | 坏 image 怎样拒绝而不是覆盖 | capacity/layout/root validation，reserved blocks 不分配 |
| 11 | [41A inode read](41-inodes.md) | offset 怎样跨 direct blocks | EOF/4097-byte 跨块/格式错误检查 |
| 12 | [41B inode write](41-inodes.md) | 多 block 扩展失败怎样不留半成品 | WritePlan、全资源 reservation、zero-before-use、runtime rollback |
| 13 | [42A path lookup](42-directories.md) | `/a/b` 怎样逐级到 inode | grammar、64-byte active entries、type/bitmap/name 校验 |
| 14 | [42B create/mkdir](42-directories.md) | child/parent 怎样正常发布 | CreatePlan、parent size 最后发布、768-entry 边界 |
| 15 | [43A file fd API](43-file-api.md) | fd/OpenFile/inode 怎么分层 | shared offset、copyout 后提交 offset、global-fsync 限制 |
| 16 | [43B NEX1 exec](43-file-api.md) | disk bytes 怎样安全变 user image | exact NEX1 header/checksum/layout，复用 exec candidate/commit |
| 17 | [43C shell + 2 boots](43-file-api.md) | 用户怎样真正使用持久文件 | ls/mkdir/touch/write/cat/sync、same-image reboot、disk program |

---

# 贯穿整个 Stage 7 的六层“完成”

任何时候都不要只写一句：

```text
“写盘成功”
```

要知道当前证据在哪一层：

```text
L1: cache memory changed
L2: cache block dirty
L3: VirtIO WRITE completion OK
L4: global needs_flush=true
L5: VirtIO FLUSH completion OK
L6: 关闭 QEMU、同一 image 重启后 READ 相同 bytes
```

只有 L6 才是本课程在这套 QEMU/backend 配置下的跨启动持久化实验。

即使达到 L6，也不等于真实硬件任意断电下已经事务一致；Stage 8 专门解决 crash consistency。

---

# 设备层不变量

## 只写受保护 regular-file image

storage helper 必须：

- 目标在项目 test-output；
- 是新建/明确测试 regular file；
- 不接受 `/dev/...`；
- 默认不覆盖；
- 不跟随危险 symlink；
- probe image 与 filesystem image 分开；
- 同一时刻只有一个 writer。

## 单位永远写全

```text
byte
512-byte VirtIO sector
4096-byte fs block = 8 sectors
```

16 MiB：

```text
32768 sectors
4096 fs blocks
```

## VirtIO status/ownership

```text
reset
→ ACKNOWLEDGE
→ DRIVER
→ features
→ FEATURES_OK + readback
→ queue setup/Ready
→ DRIVER_OK
```

modern driver 要处理 `VIRTIO_F_VERSION_1`。

一次 request：

```text
CPU owns descriptors/buffers
→ publish avail + notify
→ device may access
→ used completion + fence + status
→ CPU reclaims
```

timeout 后在 device reset **确认 status=0 之前**，不能复用旧 device-owned memory。

`avail.idx/used.idx` 按 u16 wrapping 处理。

---

# cache 不变量

```text
same (device,fs_block) 最多一个 valid slot
pinned slot 不驱逐
handle 不跨 user return/Blocked 长期保留
dirty 只有 home WRITE success 后才 clean
任何 successful home WRITE → needs_flush=true
只有 FLUSH success → needs_flush=false
```

filesystem mounted 后，normal home block 读写统一走 cache，不和 direct-driver path 混用同一区域。

Stage 8 journal blocks 5..20 会建立单独明确的 transaction I/O 规则。

---

# version=1 on-disk layout

```text
0       superblock
1       block bitmap
2       inode bitmap
3..4    inode table
5..20   reserved journal area
21      root directory data block
21..4095 data-capable region
```

初始：

```text
blocks 0..21 allocated/reserved
inode 0 reserved
inode 1 root DIRECTORY
root size=0
direct[0]=21
```

inode/dir entry 都固定 64 bytes；全部持久整数 little-endian；不直接落 Rust struct memory。

Stage 7 不解释 journal blocks。

---

# 正常运行失败 ≠ crash transaction

Lesson 41/42 的 WritePlan/CreatePlan 可以保证：

```text
参数/容量/内存资源不足
在 home I/O 发生前
→ 回滚本次 reservation
```

但一旦不同 dirty home blocks 开始 writeback：

```text
QEMU crash
```

可能只落了一部分。

所以 Stage 7 明确**没有**：

- atomic create；
- atomic multi-block write；
- atomic mkdir；
- fsck repair；
- journal recovery。

不要因为一次正常 `sync + reboot` 成功就声称已有 crash safety。

设备 WRITE/FLUSH 出错进入 filesystem `WriteUncertain/read-only error mode`；不靠恢复内存旧值冒充磁盘 rollback。

---

# user file API 不变量

```text
fd entry
→ OpenFile(ref-counted, shared offset)
→ inode
```

- 每次独立 open → 新 OpenFile/offset；
- fork/dup2 → 共享 OpenFile/offset；
- Thread → 共享 Process fd table；
- read/readdir：copyout 成功后才推进 offset；
- regular write：copyin + FS write 成功后推进；
- `fsync(fd)` 当前只是 fd 验证 + global sync，必须明确限制；
- `open(CREATE)` 在真正 create 前预留 fd/OpenFile capacity。

NEX1 checksum 只是 accidental-corruption check，不是 code signature/trust。

---

# 计划产物

```text
docs/fs-format.md
src/block/...
src/block_cache.rs
src/fs/...
tools/mkdisk.py
tools/mkfs.py
tools/dumpfs.py
tools/mknex.py
tests/storage.sh
```

依真实代码目录做合理模块复用，不为了和课程名字一致强行拆文件。

## `tests/storage.sh` 必须分阶段

```text
prepare fresh image         # 仅一次
boot1 write/create/sync
boot2 same-image read/exec  # 禁止重新 mkfs
host read-only dump/verify
```

失败保留 image/path/log，方便离线排查。

测试还要覆盖：

- VirtIO status failure；
- descriptor/index reuse；
- timeout reset；
- flush unsupported/failure；
- cache dirty writeback failure；
- bad superblock/layout；
- allocator reservation rollback；
- inode cross-block/OutOfSpace；
- bad directory/path；
- fd/OpenFile pool exhaustion；
- bad NEX1；
- 旧 boot/user/scheduling/memory/process/concurrency 全回归。

---

# 第七阶段总验收

### device

- [ ] protected raw image + exact geometry。
- [ ] modern VirtIO init/READ/WRITE/status/index/timeout ownership 正确。
- [ ] FLUSH 真正协商/提交，backend cache 配置记录。
- [ ] same-image second boot readback 成功。

### cache/format

- [ ] pin/key/dirty/needs_flush 不变量正确。
- [ ] mkfs/dumpfs/kernel mount 共用 exact format。
- [ ] bad format 只拒绝，不 auto-format。
- [ ] journal reserved blocks 从未被 Stage 7 allocator 使用。

### filesystem

- [ ] inode EOF/cross-block/max-size/OutOfSpace 正确。
- [ ] path grammar/directory entry/type/bitmap 校验正确。
- [ ] create/mkdir 普通资源失败无 reservation 泄漏。
- [ ] 明确承认 Stage 7 无 crash atomicity。

### user

- [ ] fd/OpenFile shared-offset 语义正确。
- [ ] `ls/mkdir/touch/write/cat/sync` 可以真实使用 disk FS。
- [ ] NEX1 disk program 可加载，坏 image 不破坏 old process。
- [ ] Boot1/Boot2 同 image + host dumpfs 三方一致。

通过后不要直接庆祝“文件系统完成”。下一问题正是：

> **如果 crash 发生在多个 home blocks 只写了一半时，磁盘还能不能 mount？**

进入 [第八阶段：崩溃一致性与恢复](stage-08.md)。