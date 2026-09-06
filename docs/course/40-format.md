# 第 40 课：约定磁盘上每个区域的含义

状态：待开始。前置：[第 39 课](39-cache.md) 验收完成。分 A、B 两次，每次 45～60 分钟。

## 文件系统从“约定”开始

到目前为止，磁盘只是一串 4 KiB blocks。

如果宿主机 `mkfs` 认为：

```text
block 3 是 inode table
```

而 kernel 认为：

```text
block 3 是 data
```

两边代码都可能“运行正常”，但数据一定互相破坏。

所以本课的核心产物不是先写 allocator，而是：

> **一份逐字段、逐偏移、逐字节宽度明确的 on-disk format 文档，宿主机工具和 kernel 都按它编码/解码。**

阅读 OSTEP 第 40 章。

---

# A 次：先把格式写成不可含糊的 `docs/fs-format.md`

## 1. 固定几何

本课程第一版 filesystem image：

```text
image size  = 16 MiB
sector size = 512 bytes
fs block    = 4096 bytes = 8 sectors
total blocks = 4096
```

所有 on-disk block number 都是：

```text
4 KiB filesystem block number
```

不是 VirtIO sector number。

block driver 边界才做：

```text
fs block N → sector N*8
```

---

## 2. 固定区域布局

```text
block 0      superblock
block 1      block allocation bitmap
block 2      inode allocation bitmap
block 3..4   inode table
block 5..20  reserved journal area（Stage 8 才解释）
block 21     root directory data block
block 21..4095 data-capable region
```

注意：

```text
data_start = 21
```

但格式化后 block 21 已经分给 root directory，所以普通 allocator 第一个可返回的数据 block 实际通常是 22。

初始化 bitmap 必须把：

```text
blocks 0..21 inclusive
```

全部标记已占用。

Stage 7 **绝不分配、解释或覆盖 blocks 5..20**。

---

## 3. 所有持久整数明确小端编码

on-disk 文件不能通过：

```rust
write(&rust_struct as *const _ as bytes)
```

直接落盘。

原因：

- Rust struct padding 不是磁盘协议；
- 内存布局可能因类型/编译器改变；
- 端序必须明确；
- reserved bytes 需要稳定值。

统一使用：

```text
u16 little-endian
u32 little-endian
u64 little-endian（如果未来字段需要）
```

宿主机和 kernel 都用显式 `put_u32_le/get_u32_le` 之类函数。

---

## 4. superblock 给出精确字段表

课程第一版可以固定下面这个最小 header，剩余 bytes 全部为 0：

| offset | size | field | value / meaning |
| ---: | ---: | --- | --- |
| 0 | 4 | magic | `0x314f454e`，磁盘 bytes 为 `NEO1` |
| 4 | 4 | version | `1` |
| 8 | 4 | block_size | `4096` |
| 12 | 4 | total_blocks | `4096` |
| 16 | 4 | block_bitmap_block | `1` |
| 20 | 4 | inode_bitmap_block | `2` |
| 24 | 4 | inode_table_start | `3` |
| 28 | 4 | inode_table_blocks | `2` |
| 32 | 4 | journal_start | `5` |
| 36 | 4 | journal_blocks | `16` |
| 40 | 4 | data_start | `21` |
| 44 | 4 | root_inode | `1` |
| 48..4095 | — | reserved | 全 0 |

这里的 magic 数字来自 ASCII bytes：

```text
4e 45 4f 31 = "NEO1"
```

按 little-endian 读成 `u32` 即 `0x314f454e`。

以后格式升级修改 `version`，不要“字段变了但 version 仍 1”。

---

## 5. inode 固定 64 bytes

inode number：

```text
0..127
```

共 128 个：

```text
128 * 64 = 8192 bytes = 2 blocks
```

所以恰好放在 blocks 3..4。

### inode layout

| offset | size | field |
| ---: | ---: | --- |
| 0 | 4 | type |
| 4 | 4 | size bytes |
| 8 | 48 | direct[12]，每项 u32 block number |
| 56 | 8 | reserved=0 |

课程 type：

```text
0 = FREE/unused
1 = REGULAR
2 = DIRECTORY
```

### direct block 规则

```text
0 = no block allocated
其他值 = filesystem block number
```

单文件最多：

```text
12 * 4096 = 49152 bytes
```

第一版没有 indirect block / sparse file。

---

## 6. directory entry 固定 64 bytes

| offset | size | field |
| ---: | ---: | --- |
| 0 | 4 | inode number |
| 4 | 2 | name_len |
| 6 | 1 | type |
| 7 | 1 | reserved=0 |
| 8 | 56 | name bytes |

规则：

```text
inode=0 → empty slot
name_len 1..56 for active entry
name bytes 不要求 NUL 结尾
剩余未使用 name bytes = 0（mkfs/create 时清零）
type 1=REGULAR, 2=DIRECTORY
```

active entry 的 type 必须和目标 inode.type 一致；第 42 课 lookup 会验证，而不是盲信目录项。

---

## 7. bitmap 也写精确

### block bitmap：block 1

需要 4096 bits：

```text
4096 / 8 = 512 bytes
```

定义：

```text
bit N = 1 → fs block N allocated/reserved
bit N = 0 → free
```

block 1 的前 512 bytes 有效，剩余 3584 bytes 格式化时置 0。

初始化：

```text
bits 0..21 = 1
bits 22..4095 = 0
```

### inode bitmap：block 2

128 bits = 16 bytes。

```text
bit 0 = 1  # inode 0 保留，永不分配
bit 1 = 1  # root inode
bit 2..127 = 0
```

剩余 block bytes 全 0。

---

## 8. root inode/root block 的精确初态

```text
inode 1:
  type = DIRECTORY
  size = 0
  direct[0] = 21
  direct[1..11] = 0
  reserved = 0

block 21:
  全 0 directory entries
```

为什么 `size=0` 但 direct[0]=21？

因为课程选择**提前给 root 保留第一块目录容量**，但里面当前没有任何 active entry。

目录 `size` 表示当前逻辑目录数据长度/已发布 entry 区间；第 42 课会把它要求为 64-byte entry 的整数倍。

这不是唯一文件系统设计，只是当前格式的明确约定。

---

# 宿主机 `mkfs.py`

## 安全规则

复用第 36 课：

- 只创建项目 test-output 中的新 regular file；
- 默认拒绝覆盖；
- 不接受 `/dev/...`；
- 不跟随危险 symlink；
- 创建 exactly 16 MiB。

probe image 和 filesystem image 使用不同路径。

## 格式化顺序

1. 创建/清零 16 MiB image；
2. 写 superblock；
3. 写 block bitmap；
4. 写 inode bitmap；
5. 写 inode table（root inode=1）；
6. blocks 5..20 保持全 0 且 bitmap 已占用；
7. block 21 清零；
8. flush/close host file。

## 独立 decoder

另外写：

```text
tools/dumpfs.py
```

只读解析：

```text
superblock
allocated block bits
allocated inode bits
root inode
root directory entries
```

不要让 `mkfs.py` 调用自己的 encode object 然后直接打印“看起来正确”。decoder 重新按 offsets 从 image bytes 解析。

### A 次验收

- [ ] `docs/fs-format.md` 字段/offset/width/endianness 完整。
- [ ] inode/dir entry 恰好 64 bytes。
- [ ] bitmap 位定义明确。
- [ ] root inode/block 初态与 bitmap 一致。
- [ ] journal 5..20 reserved 且 Stage 7 不分配。
- [ ] dumpfs 能独立读回 mkfs 结果。

---

# B 次：kernel mount 与 allocator

## mount 先把磁盘当“不可信 bytes”验证

读取 block 0 后，不要因为 magic 对了就直接用所有字段。

至少验证：

```text
magic == NEO1
version == 1
block_size == 4096
total_blocks > 0
total_blocks <= device_capacity_sectors / 8
```

然后 checked 验证所有固定区域：

```text
block_bitmap_block == 1
inode_bitmap_block == 2
inode_table_start == 3
inode_table_blocks == 2
journal_start == 5
journal_blocks == 16
data_start == 21
root_inode == 1
```

当前 version=1 是固定布局，和格式文档不一致就拒绝；不要“尽量猜着兼容”。

还验证：

```text
所有 block index < total_blocks
区域无非法重叠
journal end = 21
```

## bitmap 自一致性

mount 时至少检查：

```text
block bits 0..21 都为 allocated
inode bits 0,1 都 allocated
root inode type=DIRECTORY
root direct[0]=21
root size % 64 == 0
root size <= 12*4096
```

并检查 root 使用的 block 21 在 block bitmap 中确实 allocated。

第一版对 malformed image：

```text
mount error
→ 不自动 mkfs
→ 不修改 image
```

这条非常重要：坏磁盘不能因为“挂载失败”就被自动格式化覆盖。

---

## block allocator

只搜索：

```text
block >= data_start
```

但仍以 bitmap 为最终准则，所以：

```text
block 21 已 allocated → 跳过
第一个 free 通常 22
```

分配：

```text
find free bit
→ set allocated in cached bitmap
→ 新 block 清零
→ return block no
```

为什么先清零再交给 filesystem：避免新文件读取到旧文件残留。

### 失败/回滚

如果 bitmap bit 已在 cache 中设置，但清零 block 的 device/cache 操作失败，在尚未完成用户可见提交前可以恢复内存 cache bitmap 并报告 error；如果已经出现底层 I/O uncertainty，就遵循第 38/39 课的 filesystem error/read-only policy，不声称磁盘事务已经回滚。

第 44～47 课才解决 crash atomicity。

## inode allocator

搜索：

```text
inode 2..127
```

inode 0/1 永远跳过。

新 inode 交付前：

```text
64 bytes 全 0
→ type/size/direct 都从干净状态开始
```

release 只用于：

- 当前 syscall 正常失败回滚；
- 测试；

第一轮还没有用户 delete/unlink。

## 小范围耗尽测试

不要真把整张 16 MiB image 填满。

为 allocator test 限定一个小逻辑可用窗口：

```text
分配到窗口耗尽
→ 明确 OutOfSpace
→ rollback/release 测试资源
```

结束后 bitmap 回到 baseline。

## sync + reboot

修改 allocator bitmap 后：

```text
cache sync
→ home writes
→ FLUSH
→ 关闭 QEMU
→ 用同一 fs image 重启
→ mount
→ allocator bits 与预期一致
```

不要第二次启动前重新 mkfs。

## 常见问题

| 现象 | 先检查 |
| --- | --- |
| host dump 正常，kernel mount 读错 | endianness/offset 是否真共用 format doc |
| allocator 返回 block 5 | journal reserved bits/search range 是否错误 |
| allocator 返回 block 21 | root block bit 是否正确占用 |
| inode 1 被分配给普通文件 | inode search 是否从 2 开始 |
| 坏 magic 后 image 变成空文件系统 | mount failure 是否偷偷 auto-format |
| 16 MiB image 但 superblock 声称更大 | total_blocks 是否和 device capacity 交叉验证 |

## 最终验收

### format

- [ ] mkfs/dumpfs/kernel 三方使用同一 offset/endianness 定义。
- [ ] superblock、bitmap、inode、dir entry 字节布局完全明确。
- [ ] blocks 0..21 / inodes 0..1 初始占用正确。

### mount

- [ ] bad magic/version/size/layout/root/bitmap 都拒绝且不修改 image。
- [ ] total_blocks 不超过真实 block device capacity。
- [ ] root inode/block/bitmap 相互一致。

### allocator

- [ ] block allocator 不返回 reserved/journal/root block。
- [ ] inode allocator 不返回 0/1。
- [ ] 新 block/inode 交付前清零。
- [ ] 小窗口耗尽/回滚可复现。
- [ ] sync + same-image reboot 后 bitmap 保持。

### 理解验收

不看正文回答：

1. 为什么不能直接把 Rust struct memory 写成磁盘格式？
2. 为什么 block 21 属于 data region 却不能被 allocator 返回？
3. root size=0 和 direct[0]=21 为什么不矛盾？
4. 为什么 mount 必须拿 superblock total_blocks 和真实 device capacity 对比？
5. 为什么 mount 失败绝不能自动 mkfs 未知 image？
6. Stage 7 为什么要把 blocks 5..20 一直保留不用？

## 下一课为什么自然出现

现在知道“哪块空闲、哪个 inode 空闲”，但 inode 还只是一个有 12 个 block number 的 64-byte record。

下一课实现：

```text
file offset
→ direct[index]
→ fs block
→ cache
→ bytes
```

进入 [第 41 课：把磁盘块组织成文件](41-inodes.md)。