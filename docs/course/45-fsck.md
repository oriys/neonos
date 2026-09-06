# 第 45 课：让检查器指出磁盘哪里不一致

状态：待开始。前置：[第 44 课](44-crash-consistency.md)。分 A、B 两次，每次 45～60 分钟。

## 目标

实现一个宿主机只读检查器 `tools/fsck.py`。它不依赖内核运行，也不自动修复磁盘；它直接按照 `docs/fs-format.md` 解码镜像，并把违反文件系统结构不变量的位置说清楚。

本课要先明确一个边界：

> **fsck 回答“磁盘结构是否自洽”，不单独回答“某个已经 commit 的事务最终应该是 old 还是 new”。**

例如一个文件系统完全恢复到事务前 old state 时，fsck 可以是 clean；完整恢复到事务后 new state 时，fsck 也可以是 clean。

因此第 47 课会同时使用三种 oracle：

```text
journal state
+ structural fsck
+ semantic old/new state
```

不能拿“fsck 通过”代替事务语义验证。

---

## A 次：先完整扫描

1. 读取并校验超级块：魔数、版本、块大小、总块数、区域边界和 root inode。
2. 读取 block bitmap 与 inode bitmap，建立“声明已占用”的集合。
3. 遍历所有已分配 inode：验证类型、大小、direct block number 范围和最大文件大小。
4. 遍历 directory entries：验证 name length/type/inode range，并确认目标 inode 已分配。
5. 建立“实际被引用的数据块”集合，并记录引用来源，例如 `inode 7 direct[2]`。

检查器遇到一个错误后继续扫描其他独立区域，最后按错误类别汇总；但如果 superblock 本身无法安全确定范围，应停止深度扫描，避免按损坏长度越界读取镜像。

### fsck 默认必须只读

运行前后记录 image hash：

```text
hash_before == hash_after
```

检查器不能为了“让下一条检查继续”偷偷修改 bitmap/inode。

自动 repair 是另一门课，本阶段先把诊断证据做好。

---

## B 次：把不变量变成检查规则

至少实现：

- inode 引用 block 越界。
- inode 引用 reserved/journal block。
- 两个不允许共享的 inode 引用同一 data block。
- inode 引用 block 但 block bitmap 标为空闲。
- block bitmap 标占用但没有 system-reserved 用途、inode 或 directory structure 引用。
- directory entry 指向未分配 inode。
- inode bitmap 标空闲但被 directory entry 引用。
- regular file `size` 需要的 logical block 缺少 direct pointer。
- root inode 不存在或不是 DIRECTORY。

对于“bitmap 占用但无人引用”，要排除：

```text
superblock
bitmap blocks
inode table
journal blocks 5..20
root directory 初始 block
```

等固定系统用途，不能把合法 reserved block 误报成泄漏。

### 还要检查目录的 version=1 规则

当前没有 delete/free-slot 语义，所以：

```text
directory [0,size) 内每个 64-byte entry 都必须 active
```

因此可加入：

- directory size 必须是 64 的整数倍；
- logical active prefix 内 `inode=0` → corruption；
- entry type 必须与目标 inode.type 一致；
- 同一 directory 内 duplicate active name → corruption。

这让 fsck 和第 42 课的 mount/lookup 规则保持一致。

---

## 构造可重复坏镜像

不要只依赖第 44 课一次 crash 的偶然结果。增加宿主机测试工具，对**基线镜像副本**做受控 byte patch，分别制造：

```text
1. 清掉一个仍被 inode 引用的数据块 bitmap bit
2. 把两个 inode 的 direct pointer 改成同一 block
3. 把 directory entry inode 改成未分配项
4. 把 file size 改到需要一个不存在的 direct block
5. 在 directory logical prefix 中制造空 entry / duplicate name
```

每个坏镜像尽量只制造一种主要错误，方便确认 checker 真正定位目标 invariant，而不是碰巧因为另一个损坏先失败。

## 输出要求

错误不要只打印：

```text
filesystem corrupted
```

至少包含：

```text
error kind
inode number / directory entry
block number
expected relation
actual relation
```

示例形状：

```text
BLOCK_BITMAP_MISMATCH: inode=7 direct[0] -> block=25, bitmap says free
DUPLICATE_BLOCK: block=31 referenced by inode=4 direct[1] and inode=9 direct[0]
```

错误输出应尽量稳定排序，便于自动测试，不依赖 Python set/dict 的偶然遍历顺序来定义教材答案。

---

## 和 Stage 8 journal 的关系

第 46～47 课的 journal recovery 按 [统一协议](stage-08-protocol.md) 工作。

恢复后 fsck 用来证明：

```text
block/inode/directory structure 自洽
```

但还必须另外验证 semantic oracle，例如：

```text
commit 前 crash → /crash 不存在
commit 后 crash → /crash 存在
```

或者 file-write transaction 的 exact old/new bytes。

一个“结构完全 clean、但 commit 后却保留 old bytes”的镜像仍然是**恢复协议错误**，不能因为 fsck 零报错就判通过。

## 验收

- [ ] 第七阶段正常镜像扫描后零结构错误。
- [ ] 人工坏镜像分别出现预期 error kind 且位置正确。
- [ ] 第 44 课 crash 镜像能由 checker 解释，而不是只表现为“mount failed”。
- [ ] checker 不修改输入 image；运行前后 hash 相同。
- [ ] bad superblock 不诱导工具按错误边界越界读取。
- [ ] directory version=1 active-prefix/type/name 规则与第 42 课一致。
- [ ] 能解释“fsck clean”和“transaction semantic correct”为什么是两个不同断言。

本课只负责发现结构问题。下一课：[第 46 课：先把最终块镜像写进日志，再修改 home](46-journal.md)。