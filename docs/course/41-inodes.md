# 第 41 课：把磁盘块组织成文件

状态：待开始。前置：[第 40 课](40-format.md) 验收完成。分 A、B 两次，每次 45～60 分钟。

## inode 解决的核心问题

第 40 课已经固定：

```text
inode.size
inode.direct[12]
```

现在要把：

```text
(file offset, length)
```

翻译成：

```text
logical file block
→ inode.direct[index]
→ filesystem block
→ block cache
→ bytes
```

第一版只有 direct blocks，没有 sparse file、indirect block、truncate、delete。

最大文件：

```text
12 * 4096 = 49152 bytes
```

---

# A 次：先实现完全只读的 inode/file path

## 1. 读取 inode 前先验证磁盘记录

`read_inode(inum)` 至少检查：

```text
inum < 128
inode bitmap bit == allocated
inode.type 是 REGULAR 或 DIRECTORY
inode.size <= 49152
reserved bytes == 0（version=1 基线）
```

如果是 regular file，再逐个验证它声明使用到的 direct block：

```text
block number < total_blocks
block number >= data_start
block bitmap bit == allocated
```

不要因为 inode bytes 来自自己的 image 就完全信任它；第 45 课 fsck 会做更全局的交叉一致性检查，本课先做单 inode 的基本防越界。

## 2. `read_at` 统一使用 end-exclusive 语义

概念接口：

```text
read_at(inode, offset, dst)
```

先处理：

```text
if offset >= inode.size:
    return 0
```

然后：

```text
remaining_file = inode.size - offset
want = min(dst.len, remaining_file)
```

不用：

```text
offset + len - 1
```

避免 zero-length/overflow 边界。

## 3. 每段怎样定位

对当前位置 `pos`：

```text
logical_index = pos / 4096
offset_in_block = pos % 4096
chunk = min(
    remaining request,
    4096 - offset_in_block
)
```

要求：

```text
logical_index < 12
```

再取：

```text
home_block = inode.direct[logical_index]
```

### 当前不支持 sparse file

如果：

```text
pos < inode.size
```

但相应：

```text
direct[logical_index] == 0
```

这不是“自动读零”，而是：

```text
FilesystemCorrupt / FormatError
```

因为 version=1 规定已声明文件大小范围内的数据 block 必须存在。

## 4. 通过 cache 读，不绕过第 39 课

```text
get_block(device, home_block)
→ pin handle
→ copy chunk 到 kernel dst
→ release handle
```

不要直接调用 VirtIO driver 读同一 filesystem home block；否则 cache 可能保留 stale copy。

## 5. 用 4097-byte pattern 抓跨块错误

宿主机离线工具预置一个 regular file：

```text
size = 4097
```

内容用确定 pattern，例如：

```text
byte[i] = i % 251
```

验证：

- offset 0 读前若干 bytes；
- offset 4095 读 2 bytes；
- offset 4096 读最后 1 byte；
- offset 4097 / 更大 → 0；
- 精确 4096-byte 文件；
- empty file。

### A 次验收

- [ ] offset>=EOF 返回 0，不越界读其他 block。
- [ ] 跨 block chunk 计算正确。
- [ ] 已声明 size 内 direct=0 被当格式错误，不冒充 sparse。
- [ ] data block 范围/bitmap 基本校验存在。
- [ ] 所有读取走 block cache。

---

# B 次：覆盖、扩展和资源不足

## 1. 第一版写入不允许造洞

概念：

```text
write_at(inode, offset, src)
```

规则：

```text
0 <= offset <= inode.size
```

所以允许：

- 覆盖现有 bytes；
- 从 EOF append；
- 从现有区域开始并连续扩展越过旧 EOF。

不允许：

```text
offset > inode.size
```

因为那会产生 hole/sparse 语义，当前未实现。

zero-length：

```text
return 0
```

不分配 block、不改 inode。

## 2. 最终大小用 checked arithmetic

```text
end = offset.checked_add(src.len)
new_size = max(old_size, end)
```

必须：

```text
new_size <= 49152
```

超过：

```text
FileTooLarge
```

第 43 课用户 syscall 再额外限制单次 user read/write <=256 bytes；本课内部 inode helper 不需要把“用户 ABI 限制”混进 on-disk 逻辑。

## 3. 先生成 `WritePlan`，再修改

为了让普通资源不足失败不留下半成品，先计算：

```text
影响哪些 logical blocks
哪些 direct entry 已存在
哪些需要新 block
旧 size/new size
```

例如 offset=4095 写 2 bytes：

```text
可能同时碰 logical block 0 和 1
```

如果 block 1 尚不存在，就先预留一个新 data block。

### 预留全部新资源

在真正修改 file data/inode 前：

1. 确认 logical index <=11；
2. 预留所有需要的新 block number；
3. 新 block 清零；
4. 取得/确认所有需要的 cache handle/metadata 容量；
5. 保存本次 reservation list 供正常失败回滚。

如果第 2 个新 block 分配失败：

```text
释放第 1 个 reservation
恢复 bitmap cache 状态
inode/direct/data 仍保持调用前
```

不要边写第一个 data block 边去申请第二个。

## 4. 新 block 必须先是全零

假设只从新 block offset=100 写 20 bytes。

未覆盖的：

```text
0..99
120..4095
```

都应该保持 0，而不是旧磁盘残留。

第 40 课 allocator 的 zero-before-use 正好提供这条保证。

## 5. 在内存 cache 中一次提交当前写操作

资源全部准备好后：

```text
修改受影响 data cache block bytes
→ mark dirty
→ 更新 inode.direct 新指针
→ 更新 inode.size
→ mark inode-table block dirty
→ bitmap block 保持 reservation dirty
```

当前 kernel 单 hart、filesystem syscall 串行，因此用户在 syscall 返回前不会并发观察半写状态。

### 一个重要限制：这还不是 crash transaction

这些 dirty blocks 最终可能按：

```text
data
bitmap
inode
```

或其他顺序被 cache writeback。

如果 QEMU 在中间崩溃，磁盘可能得到：

- block allocated 但 inode 没引用；
- inode 引用 block 但 bitmap/内容不同步；
- file size 与 data 更新顺序不一致。

**本课不解决它。**

第 44 课会故意把这些不一致复现出来；第 46～47 课再用 journal/recovery 修。

## 6. 普通 runtime failure 和 disk uncertainty 分开

### 在任何 home I/O 发生前的正常失败

例如：

```text
OutOfSpace
metadata capacity 不足
参数非法
```

可以 rollback 本次 reservations，保持 filesystem cache 逻辑状态不变。

### cache writeback / device I/O 已经失败

如果：

```text
某个 dirty home block WRITE/FLUSH 失败
```

遵循第 38/39 课：

```text
filesystem/device 进入 WriteUncertain / read-only error mode
```

不要把内存旧 inode 恢复后就声称磁盘也恢复到旧事务状态。

## 7. 空间不足测试

用小的 allocator test window 让一次扩展需要 2 个新 block，但只剩 1 个。

验证：

```text
write 返回 OutOfSpace
inode.size 不变
inode.direct 不变
file old bytes 不变
block bitmap 回到调用前
```

这个场景比“一个 block 都没有”更能证明多资源 reservation rollback。

## 8. sync + reboot

成功 write 后：

```text
sync()
→ 所有 dirty home write
→ FLUSH
→ 关闭 QEMU
→ 同一 image 重启
→ mount
→ read_at exact bytes
```

记录：

```text
file size
各 direct block number
content checksum
```

不要只在 cache 中立刻 read 自己刚写的内容。

## 常见问题

| 现象 | 先检查 |
| --- | --- |
| offset 4095 写 2 bytes 只改了 1 block | chunk/logical-index 计算是否正确 |
| EOF 后读出其他文件旧数据 | read 是否按 inode.size 截断 |
| 新 block 未写区域出现旧内容 | allocator 是否 zero-before-use |
| 空间不足后 inode.size 变大 | 是否先修改 inode 再完成全部 reservation |
| direct=0 被当成一页零数据 | 当前 version 根本不支持 sparse file |
| sync I/O error 后代码声称 rollback 完成 | 是否混淆内存 rollback 和磁盘已发生写入 |

## 最终验收

### read

- [ ] empty/EOF/exact-block/4097-byte 跨块读取正确。
- [ ] 已声明 file range 内缺 block pointer 报格式错误。
- [ ] block pointer 范围/bitmap 基本检查存在。

### write

- [ ] offset>size 被拒绝，不造 hole。
- [ ] checked end/max file size 正确。
- [ ] 新 block 全量预留、清零后才提交。
- [ ] 中途资源不足 rollback 无 size/direct/bitmap/data 残留。
- [ ] 成功写后 inode/data/bitmap cache 都标 dirty。

### persistence/边界

- [ ] sync+same-image reboot 后 size/content/block mapping 正确。
- [ ] 明确知道 Stage 7 write **没有 crash atomicity**。
- [ ] device write/flush error 进入错误模式，不伪装磁盘 rollback。

### 理解验收

不看正文回答：

1. `offset>=size` 为什么返回 EOF 而不是继续沿 direct blocks 读？
2. 当前 direct=0 为什么不是 sparse zero page？
3. 为什么写入要先 reserve 全部新 blocks 再修改 inode/data？
4. offset<size 的 write 什么时候可以扩展 file size？
5. zero-before-use 解决了什么问题？
6. 为什么 Stage 7 的“资源分配失败可 rollback”不等于“崩溃时文件更新原子”？

## 下一课为什么自然出现

现在 inode 能保存文件内容，但用户仍然只能靠 inode number 找它。

人真正使用的是路径：

```text
/notes/first
```

下一课要把目录也当成一种受格式约束的 inode/data，并实现：

```text
path component
→ directory entry
→ inode number
```

进入 [第 42 课：给文件取名字](42-directories.md)。