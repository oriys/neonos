# 第 42 课：给文件取名字

状态：待开始。前置：[第 41 课](41-inodes.md) 验收完成。分 A、B 两次，每次 45～60 分钟。

## 目录不是“特殊字符串表”，它也是 inode + file blocks

第 40 课已经定义每个 directory entry 固定 64 bytes：

```text
inode number
name_len
type
reserved
name[56]
```

本课把 DIRECTORY inode 的 data blocks 解释成这种 entry 数组，实现：

```text
absolute path
→ component
→ directory lookup
→ inode
```

第一版没有：

- `.` / `..`；
- current working directory；
- symlink/hard link；
- delete/unlink；
- rename；
- trailing-slash 语义。

这些缺失不是 bug，而是课程明确边界。

---

# A 次：只实现绝对路径 lookup

## 1. 先固定 path grammar

合法 path：

```text
/
/notes
/notes/first
```

限制：

```text
总长度 1..128 bytes
必须以 / 开始
/ 单独表示 root
非 root 不允许以 / 结尾
不允许 // 空 component
不允许 .
不允许 ..
每个 component 1..56 bytes
```

component 第一版只允许可打印 ASCII：

```text
0x21..0x7e
但不包含 '/'
```

因此不支持空格/NUL/控制字符。以后加入 UTF-8/quoted shell 时再扩展，不在这里把“byte length”和“字符数”混在一起。

所有长度都是：

```text
bytes
```

不是 Unicode code points。

## 2. path parser 先独立测试

先把：

```text
"/notes/first"
```

拆成：

```text
["notes", "first"]
```

不访问磁盘。

边界矩阵：

```text
/                → root, 0 components
/notes           → [notes]
/notes/first     → [notes,first]
//notes          → invalid
/notes/          → invalid
/./x             → invalid
/../x            → invalid
component 56 B   → valid
component 57 B   → invalid
path 128 B       → 按最终 bytes 验证
path 129 B       → invalid
```

这样“字符串 parser bug”和“filesystem lookup bug”不会同时调。

---

## 3. DIRECTORY inode 本身也先验证

对一个要遍历的目录：

```text
inode.type == DIRECTORY
inode.size <= 49152
inode.size % 64 == 0
```

当前 version=1 没有 delete，所以目录逻辑内容采用：

```text
0 .. inode.size
```

作为**连续 active entry prefix**。

因此在 `[0,size)` 内每个 64-byte entry 都必须：

```text
inode != 0
name_len 1..56
reserved == 0
type == REGULAR or DIRECTORY
```

`inode=0` 的 zero entry 只存在于尚未进入逻辑 size 的未使用空间；如果在 `[0,size)` 遇到 inode=0，本课把它视为格式错误，而不是偷偷跳过。

这条规则以后如果加入 delete/free slot，会升级格式/目录语义。

## 4. active entry 的所有字段都不能盲信

解析 entry 后继续验证：

```text
referenced inode < 128
inode bitmap 表明 allocated
target inode 能成功 decode
target inode.type 与 entry.type 一致
name_len 范围正确
name bytes 符合当前 ASCII 规则
```

第 40 课格式要求 unused name padding 为 0；version=1 reader 可以把非零 padding 报成格式异常，帮助尽早发现坏镜像。

## 5. lookup 一个 component 时检测重复名字

遍历当前目录所有 active entries：

```text
name == component
```

正常只能命中：

```text
0 次 → NotFound
1 次 → 返回 target inode
```

如果同一个目录里命中 2 次同名 active entry：

```text
FilesystemCorrupt
```

不要随便取“第一个”，把磁盘不一致藏起来。

## 6. 多级 lookup

```text
current = root inode 1
for component in path:
    current 必须是 DIRECTORY
    current = lookup_component(current, component)
return current
```

例如：

```text
/notes/first
```

如果 `/notes` 存在但它是 REGULAR：

```text
TypeMismatch
```

不是：

```text
NotFound
```

这两个错误对调试/用户反馈含义不同。

### A 次验收

- [ ] path parser 和 disk lookup 是两层测试。
- [ ] root `/` 不读取不存在的 component。
- [ ] directory size 必须是 64-byte multiple。
- [ ] `[0,size)` 不接受空 entry。
- [ ] entry inode/type/bitmap/name 全部交叉验证。
- [ ] duplicate names 被当 corruption。
- [ ] NotFound 与 TypeMismatch 区分。

---

# B 次：创建文件、mkdir 和列举

## 1. 先计算目录容量

一个 directory 最多和普通 file 一样 12 个 direct blocks：

```text
12 * 4096 = 49152 bytes
```

每 entry 64 bytes：

```text
49152 / 64 = 768 entries
```

所以 version=1 单目录最大：

```text
768 active entries
```

满了返回明确 `DirectoryTooLarge/NoSpace`，不越界写第 13 个 direct pointer。

## 2. create/mkdir 先拆成 parent path + leaf

例如：

```text
/notes/first
```

先得到：

```text
parent = /notes
leaf   = first
```

先 lookup parent，并确认 DIRECTORY。

再查 leaf：

```text
已存在 → NameExists
不存在 → 才继续创建
```

查重、资源预留、最终发布都在当前单 hart filesystem 串行 syscall 中完成，所以不会有两个 Thread 同时成功创建同名 entry。

---

## 3. 创建前先生成 `CreatePlan`

### regular file 需要

```text
1 个 free inode
parent directory 可能需要 1 个扩展 block
```

新 regular inode 初始：

```text
type = REGULAR
size = 0
direct[] = 0
```

### mkdir 需要

```text
1 个 free inode
1 个 child directory data block（提前分配第一块）
parent directory 可能需要 1 个扩展 block
```

新 directory inode：

```text
type = DIRECTORY
size = 0
direct[0] = child_block
其余 direct=0
```

child directory block 全 0。

这和 root inode 的“size=0 但 direct[0] 已有一块”规则一致。

### parent 何时需要扩展 block

新 entry 放在：

```text
entry_offset = parent.size
```

如果它进入新的 logical directory block，而对应 `parent.direct[index]==0`，就预留一个新 block。

所有 block/inode reservation 在修改 parent/child 之前全部完成。

---

## 4. 正常运行时的发布顺序

资源都准备好后，逻辑上：

```text
1. 初始化 child inode
2. mkdir 时初始化 child directory block
3. 必要时安装 parent 新 direct block
4. 在 parent data block 写完整 64-byte dir entry
5. 最后 parent.size += 64
```

为什么 `parent.size` 最后：

```text
size
```

决定 reader 会解析到哪些 entry。

在当前单 hart 串行 kernel 中，只要 size 还没增加，其他用户 syscall 不会把末尾未发布 slot 当 active entry。

### 但这不是 crash ordering 保证

这些修改都只是不同 cache blocks 的 dirty state。

Stage 7 `sync()` 最终 writeback 顺序并不构成事务；如果 QEMU 在中间崩溃，仍可能出现：

- inode allocated 但 parent entry 不存在；
- parent entry 指向 inode，但 bitmap/child block 状态没同步；
- parent size 已 durable，但 entry block 未 durable。

第 44 课会故意复现这些情况。

“parent.size 最后修改”只帮助**正常运行时发布语义**，不是断电原子性。

---

## 5. 正常失败回滚

在任何本次修改对外发布前，如果：

```text
inode 不足
block 不足
cache metadata/resource 不足
参数非法
```

回滚：

```text
child inode reservation
child block reservation
parent expansion block reservation
```

保持：

```text
parent.size 不变
parent existing entries 不变
```

如果底层 device write/flush 已经进入 uncertainty，则按第 38/39/41 课切 filesystem read-only/error mode，不声称已经把磁盘历史写入“撤销”。

---

## 6. `readdir/list` 只读取逻辑 active prefix

遍历：

```text
0,64,128,... < dir.size
```

每个 entry 走 A 次同样 validation。

因为当前没有 delete：

```text
inode=0 inside logical size → corruption
```

不要为了“列表还能显示”就跳过坏 slot。

输出顺序就是 directory entry 顺序；当前不排序名字。

## 7. 固定联调

执行：

```text
mkdir /notes
create /notes/first
写 first 内容
sync
关闭 QEMU
同一 image 重启
```

第二次：

```text
lookup /notes
readdir /notes
lookup /notes/first
read /notes/first
```

验证：

```text
name → inode number → inode content
```

三层一致。

## 常见问题

| 现象 | 先检查 |
| --- | --- |
| `/a//b` 被接受 | parser 是否允许空 component |
| 找到 entry 后越界 inode | 是否验证 inode number/bitmap |
| 同名出现两个文件 | create 查重/发布是否真串行 |
| mkdir 后 child lookup 读到垃圾 | child block 是否 zero-before-publish |
| parent size 增了但 entry 是全 0 | 是否在正常逻辑里提前 publish size |
| create 失败后 inode 数越来越少 | reservation rollback 是否完整 |
| 坏目录 entry 被 readdir 静默跳过 | 当前 no-delete 格式不应容忍 logical prefix 空洞 |

## 最终验收

### lookup

- [ ] path grammar 边界全部有确定结果。
- [ ] root、多级、NotFound、TypeMismatch 正确。
- [ ] directory entry/inode/bitmap/type 交叉校验。
- [ ] duplicate name/corrupt slot 被明确拒绝。

### create/mkdir

- [ ] 所有 inode/block 先 reservation，再修改。
- [ ] regular 与 directory 初始 inode 不混淆。
- [ ] mkdir child block 先清零。
- [ ] parent entry 完整写好后才增加 logical size。
- [ ] 资源不足 rollback 不残留 inode/block/entry。
- [ ] 单目录最多 768 entries。

### persistence/边界

- [ ] `/notes/first` sync + same-image reboot 后可 lookup/read。
- [ ] 明确知道正常发布顺序不等于 crash atomicity。

### 理解验收

不看正文回答：

1. 为什么 directory size 必须是 64 的倍数？
2. 当前没有 delete 时，logical size 内 inode=0 为什么算 corruption？
3. entry.type 为什么还要和 target inode.type 对比？
4. mkdir 为什么比 create regular file 多需要一块 child data block？
5. 为什么 parent.size 要在正常逻辑最后增加？
6. 为什么即使这样做，Stage 7 crash 时仍可能得到不一致磁盘？

## 下一课为什么自然出现

现在 kernel 内部已经可以：

```text
lookup/create/read/write directory/file
```

但用户 shell 还没有真正的文件 fd/API，也不能从 disk file 加载程序。

下一课把 filesystem 接回用户世界：

```text
open/read/write/seek/readdir/fsync
+
exec_path
+
shell file commands
```

进入 [第 43 课：在 shell 中使用磁盘文件](43-file-api.md)。