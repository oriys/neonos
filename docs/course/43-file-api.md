# 第 43 课：在 shell 中使用磁盘文件

状态：待开始。前置：[第 42 课](42-directories.md) 验收完成。分 A、B、C 三次，每次 45～60 分钟。

## 这是 Stage 7 的“用户世界闭环”

kernel 内部已经会：

```text
mount
lookup
create/mkdir
inode read/write
sync
```

本课把它接回已经存在的 fd/process/shell 模型：

```text
user path
→ syscall
→ fd
→ OpenFile
→ inode/cache
→ disk
```

并最终让：

```text
exec_path("/bin/hello")
```

从真正的 disk file 构造新的用户 AddressSpace。

这仍是教学格式/教学 ABI，不是 POSIX/Linux ELF 接口。

---

# A 次：regular file / directory fd API

## 新 syscall

保留 1～25，原：

```text
read=9
write=10
close=11
dup2=12
```

继续按 fd object type 分发。

新增：

| a7 | 接口 | 课程语义 |
| ---: | --- | --- |
| 26 | `open(path,len,flags)` | 打开/按需创建 regular file，返回 fd |
| 27 | `seek(fd,offset)` | regular file 绝对 offset，0..size |
| 28 | `mkdir(path,len)` | 创建 directory |
| 29 | `readdir(fd,buf,len)` | 只对 directory，返回完整 64-byte records |
| 30 | `fsync(fd)` | 验证 file/dir fd 后执行**全局 filesystem sync** |
| 31 | `exec_path(path,len)` | 从 regular file 加载 NEX1 image，成功不回旧程序 |
| 32 | `sync()` | 全局 writeback + device FLUSH |

新增错误码：

```text
-11 NotFound
-12 NameExists
-13 TypeMismatch
-14 IO/Format/FilesystemError
-15 FileOrDirectoryTooLarge
```

旧：

```text
-2 invalid argument/length/flags
-3 invalid user address
-4 resource shortage
-7 bad fd/object direction
```

---

## 1. `open` flags 写成 mask，不叫“第 1/2/4 位”

```text
READ   = 0x1
WRITE  = 0x2
CREATE = 0x4
KNOWN  = 0x7
```

验证：

```text
flags & !KNOWN == 0
(flags & (READ|WRITE)) != 0
```

未知 bit → `-2`。

### CREATE 语义固定为 create-if-missing

```text
CREATE + path 不存在
→ 创建 REGULAR file

CREATE + regular file 已存在
→ 打开已有 file，不报 NameExists
```

它不是 `O_EXCL`。

`mkdir` 仍在目标已存在时返回 `-12`。

Directory 第一版只允许 READ open；对 directory 请求 WRITE → `-13`。

---

## 2. path 一律先完整 copy 到 kernel buffer

`path,len`：

```text
1 <= len <= 128
```

先：

```text
copy_from_user(path,len)
```

到固定 kernel path buffer，再按第 42 课 parser 验证。

不要在 lookup 过程中一会儿读用户 page1、一会儿 scheduler 后再读 page2。

本阶段 kernel syscall 不抢占，但“先复制成 kernel-owned bytes”仍让 API 边界更清楚。

路径不是 NUL-terminated C string；`len` 是唯一长度来源。

---

## 3. 引入 `OpenFile`：fd 不直接保存 inode+offset

全局固定 OpenFile pool：

```text
OpenFile {
  id/generation
  inode_no
  rights: READ/WRITE
  offset
  ref_count
  object_type: Regular/Directory
}
```

Process fd entry 对 file 类型保存：

```text
File(open_file_handle)
```

### 为什么需要这一层

每次独立 `open()`：

```text
新的 OpenFile
→ 独立 offset=0
```

但：

```text
fork
或 dup2
```

复制 fd entry 时：

```text
共享同一个 OpenFile
→ ref_count++
→ offset 也共享
```

同 Process Threads 本来就共享一张 fd table，因此自然看到同一个 OpenFile。

close：

```text
fd entry 删除
→ OpenFile.ref_count--
→ 0 时释放 OpenFile slot/generation
```

这和 pipe endpoint refs 是同样的“fd 是引用，不是对象本体”思想。

---

## 4. `open(CREATE)` 先预留 fd/OpenFile capacity，再真的创建 inode

错误流程：

```text
filesystem 已创建 /notes/a
→ 才发现 fd table 满了
→ open 返回 -4
→ 但文件已经偷偷出现
```

本课程要求：

```text
先 reserve 一个 fd slot
→ reserve 一个 OpenFile slot
→ 再 lookup/create filesystem object
→ 成功后 publish OpenFile + fd
```

失败释放 reservations。

对于已有 file 也先确认资源 capacity，再返回 fd。

如果 CREATE 触发的 filesystem create 在正常资源阶段失败，沿第 42 课 rollback；底层 I/O uncertainty 则进入 filesystem error mode，不假装“删回去”已经 durable。

---

## 5. regular `read`：copyout 成功后才推进 shared offset

用户：

```text
read(fd,buf,len)
```

file path：

1. fd → OpenFile，验证 READ right；
2. `len<=256`；zero len → 0，不访问 buf；
3. 从 `OpenFile.offset` 调 inode `read_at` 到固定 kernel temp；
4. 得到 `n`；
5. `copy_to_user(buf,n)`；
6. copyout 全成功后：
   ```text
   OpenFile.offset += n
   ```
7. 返回 n。

bad user pointer：

```text
-3
```

offset **不前进**。

否则同一个 shared OpenFile 在 fork/dup2 后会“因为一个坏指针把所有引用者的 offset 吃掉”。

## 6. regular `write`：先 copyin，再修改 file，再推进 offset

1. 验证 WRITE right；
2. `len<=256`；zero len →0；
3. `copy_from_user` 到 kernel temp；
4. inode `write_at(offset,temp)`；
5. 当前教学 regular-file write 要么完成这次全部 `len`，要么返回 error；
6. 成功后：
   ```text
   offset += len
   ```

pipe 的 `write` 仍保留第 27 课 partial semantics；按 fd object type 分发，不强行把两种对象做成同一种内部行为。

如果 filesystem write 进入 IO uncertainty，offset 不前进，并向用户返回 `-14`；filesystem 进入只读/error mode。

---

## 7. `seek` 只做最小绝对定位

```text
seek(fd,offset)
```

只对 REGULAR：

```text
0 <= offset <= inode.size
```

当前 ABI 参数是非负整数；不支持负 offset/SEEK_CUR/SEEK_END。

不允许：

```text
offset > size
```

因为第 41 课不支持 sparse hole。

成功：

```text
OpenFile.offset = offset
return offset
```

pipe/console/directory → `-7` 或明确 TypeMismatch，不偷偷改变状态。

---

## 8. `readdir` 使用同一个 OpenFile.offset

只对 DIRECTORY + READ right。

用户 buffer：

```text
64 <= len <= 256
```

返回长度永远是：

```text
64 的整数倍
```

计算：

```text
capacity = floor(len/64) * 64
```

从 directory logical `OpenFile.offset` 开始读取完整 active entries。

要求 offset：

```text
%64 == 0
<= directory.size
```

kernel 先按第 42 课规则验证 entry，然后把**经过验证的逻辑字段重新序列化**成 64-byte user record：

```text
inode u32 LE
name_len u16 LE
type u8
reserved=0
name[56] + zero padding
```

不要把未经验证的 raw disk cache bytes 直接 copy 给 user。

然后：

```text
copy_to_user
→ 成功后才 offset += returned_bytes
```

EOF：

```text
offset == dir.size → return 0
```

bad pointer 不推进 offset。

---

## 9. `fsync(fd)` 的名字要解释清楚

第一版：

```text
fsync(file_or_dir_fd)
```

只做：

1. 验证 fd 是有效 regular/directory OpenFile；
2. 调用第 39 课的**全局 filesystem `sync()`**；
3. home dirty writeback + VirtIO FLUSH；
4. 成功返回 0。

所以它不是成熟 POSIX “只保证这个 file 及必要 metadata”的精确 per-file fsync。

课程必须明确写：

> **`fsync(fd)` 在第一版只是 user-visible 的全局 sync 别名，fd 只用于参数/type 校验。**

真正 per-inode dependency tracking 留给进阶。

---

## A 次实验矩阵

### offset 语义

1. 同一 file `open` 两次：两个 OpenFile offset 独立。
2. `open` 后 fork：parent/child fd 共享同一个 OpenFile offset。
3. `dup2` 后两个 fd 共享 offset。
4. bad read buffer：offset 不变。
5. `readdir` bad buffer：directory offset 不变。

### CREATE/resource

- fd table 满时 `open(CREATE)` 不创建隐藏 file。
- OpenFile pool 满时同样不创建。
- CREATE existing regular → 打开已有。
- mkdir existing → -12。

### A 次验收

- [ ] fd/OpenFile/inode 三层明确。
- [ ] open/dup2/fork/close ref_count 守恒。
- [ ] regular read/write 的 user-copy 和 offset 提交顺序正确。
- [ ] readdir 只返回完整、验证后的 64-byte records。
- [ ] `fsync(fd)` 的“当前是全局 sync”限制写清楚。

---

# B 次：从磁盘加载 NEX1 教学程序

## 1. 先把 NEX1 也写进 `docs/fs-format.md`

它不是 ELF。

第一版 header 固定 32 bytes：

| offset | size | field |
| ---: | ---: | --- |
| 0 | 4 | magic，bytes=`NEX1` (`u32 LE 0x3158454e`) |
| 4 | 2 | version=`1` |
| 6 | 2 | header_len=`32` |
| 8 | 4 | code_len |
| 12 | 4 | data_len |
| 16 | 4 | bss_len |
| 20 | 4 | entry_offset（相对 code start） |
| 24 | 4 | payload_checksum32 |
| 28 | 4 | reserved=0 |

payload：

```text
[code bytes][data bytes]
```

BSS 不占文件 payload，只记录长度并由 loader 清零。

checksum 教学算法可以固定为：

```text
所有 payload bytes 做 u32 wrapping sum
```

它只用于检测意外损坏/截断，**不是密码学签名，也不能证明程序可信**。

## 2. host pack tool

计划：

```text
tools/mknex.py
```

输入第 21/24 课已经验证可重定位的用户 blob，输出 NEX1 file。

再由离线 image builder 把它作为：

```text
/bin/hello
```

放进初始 filesystem image。

不要让 kernel 为了测试 `exec_path` 先自己动态生成“正确 program file”；host side seed 让 on-disk 输入独立可核对。

---

## 3. `exec_path(path,len)` 先完全验证，再 build candidate

### path/file

```text
copy path to kernel
→ lookup
→ target 必须 REGULAR
```

读取前 32 bytes header，验证：

```text
magic/version/header_len/reserved
code_len > 0
entry_offset < code_len
```

checked 计算：

```text
payload_len = code_len + data_len
expected_file_len = 32 + payload_len
```

要求：

```text
inode.size == expected_file_len
```

不接受截断，也不偷偷忽略 trailing garbage。

再验证：

```text
payload checksum
bss/data/code memory length 不溢出
符合当前 MAX_USER_IMAGE / virtual layout
code/data/stack 不重叠
```

## 4. 权限和布局复用第 21/24 课

构造 CandidateImage：

```text
code pages → R-X,U=1
data+bss   → RW-,U=1
stack      → RW-,U=1
guard      → unmapped
kernel     → borrowed U=0
```

code/data 从 file 逐段复制到新 frames；BSS 清零。

完成 code 写入后：

```text
fence.i
```

entry：

```text
USER_CODE_BASE + entry_offset
```

## 5. 不需要把整个 48 KiB file 一次读进连续 kernel buffer

loader 可以：

```text
按小 fixed buffer 从 inode read_at
→ copy into candidate user physical frames
```

因为整个 `exec_path` syscall 在当前单 hart、S-mode 不调度的 filesystem 串行路径中执行，目标 file 在这次 loader snapshot 期间不会被另一个 user syscall 并发修改。

第六阶段的多线程限制仍然生效：多线程 Process `exec_path` → -9。

## 6. 提交完全复用第 24 课

在：

```text
header/file/checksum/layout
+
Candidate AddressSpace/context
```

全部成功之前：

```text
old Process image 完全不变
```

成功：

```text
同 PID
fd table/OpenFile refs 保留
safe kernel root 过渡
commit candidate
destroy old image
进入 new entry
```

失败：

```text
返回 -14/-4/-11/-13 等
old user program 继续执行
```

### NEX1 安全边界

NEX1 checksum 只验证 accidental corruption，不表示 `/bin/hello` 被信任。

程序仍在 U-mode 运行，安全性依赖：

```text
页表 U/R/W/X
syscall validation
kernel isolation
```

不要把“checksum 对了”写成“代码安全”。

### B 次负例

分别制作 host-side bad files：

- bad magic；
- bad version；
- truncated payload；
- trailing bytes；
- entry_offset==code_len；
- checksum mismatch；
- huge bss overflow/layout；
- resource shortage。

每个都必须：

```text
exec_path returns error
old program prints "still alive"
PID/old image unchanged
candidate frames rollback
```

---

# C 次：shell 文件命令 + 两次启动

## shell parser 从“无参数”升级成有限空格分词

支持：

```text
ls /
mkdir /notes
touch /notes/a
write /notes/a hello
cat /notes/a
sync
/bin/hello
```

第一版：

- 只按 ASCII space 分 token；
- 不支持 quotes/escape；
- path 本身不含 space；
- `write` data 只接受一个无空格 token；
- token 数不对 → usage error，不执行半个命令。

## 命令如何映射 syscall

### `touch path`

```text
open(path, WRITE|CREATE)
→ close
```

existing regular 也成功。

### `write path data`

```text
open(path, WRITE|CREATE)
→ seek 到当前 size（需要先通过 read/metadata helper 或课程 shell 简化 open 后 seek size 查询接口策略）
```

为了不额外发明 `stat` syscall，本课程更简单地定义 shell `write` 为：

> 打开/创建文件后，从 offset 0 覆盖写入给定 token；不承诺 append。

这样命令：

```text
write /notes/a hello
```

确定得到文件前 5 bytes=`hello`；如果原文件更长，本阶段没有 truncate，因此旧尾部仍可能保留。测试使用新文件，避免把“write”冒充完整文本编辑器。

### `cat path`

```text
open READ
→ read loop
→ write fd1 loop
→ close
```

### `ls path`

```text
open READ directory
→ readdir loop
→ 按 record name_len 输出
→ close
```

### `sync`

```text
syscall 32
```

### `/bin/hello`

和第 28 课 external command 一样：

```text
fork
child exec_path
parent wait
```

不暗中实现 argv；磁盘程序仍无参数。

---

## 两次启动验收必须使用同一个 seed image

### test prepare（只做一次）

host：

```text
mkfs fresh image
→ preload /bin/hello NEX1
```

记录 image path。

### Boot 1

shell：

```text
mkdir /notes
touch /notes/a
write /notes/a hello
sync
exit
```

确认：

```text
BOOT1_SYNC_OK
```

正常关闭 QEMU。

### Boot 2

**不 mkfs，不重建，不覆盖 `/bin/hello`。**

同一个 image：

```text
ls /
ls /notes
cat /notes/a
/bin/hello
```

必须：

```text
看到 notes/a
cat 输出 hello
磁盘 NEX1 程序运行并 exit
```

最后只读 host `dumpfs.py` 再核对 inode/path/content。

---

## Stage 7 最终资源/错误回归

多轮 file commands 后检查：

```text
OpenFile pool refs
Process fd entries
cache pin counts
free frames
inode/block allocation counts
pipe refs
Thread/Process slots
```

异常路径至少：

- bad path pointer；
- invalid flags；
- fd/OpenFile pool full；
- open missing without CREATE；
- write directory；
- readdir regular file；
- seek pipe；
- bad NEX1；
- device/filesystem read-only error mode。

## 常见问题

| 现象 | 先检查 |
| --- | --- |
| fork 后 parent/child 各自从 offset 0 读 | 是否错误复制 OpenFile，而非共享 ref |
| `open(CREATE)` 返回 -4 但文件已经出现 | 是否创建前没预留 fd/OpenFile capacity |
| bad read pointer 后 offset 变了 | 是否 copyout 前推进 offset |
| bad readdir pointer 跳过 entry | 同上 |
| fsync 只写当前 inode 导致别的 dirty 未处理 | 当前课程 fsync 实际定义就是 global sync，文档/实现是否一致 |
| bad NEX1 后 old process 消失 | 是否 candidate 完成前提交 exec |
| checksum 正确就被称为“可信代码” | checksum 不是认证/signature |
| Boot 2 文件都没了 | 测试是否偷偷重新 mkfs |

## 第七阶段最终验收

### fd/file API

- [ ] 每次 open 独立 offset；fork/dup2 共享 OpenFile offset。
- [ ] read/readdir 只有 copyout 成功才推进 offset。
- [ ] regular write 先 copyin，成功后推进 offset。
- [ ] `open(CREATE)` resource reservation 不产生隐藏文件。
- [ ] invalid flags/type/fd/pointer 全有确定错误。

### NEX1

- [ ] header/file length/checksum/entry/layout 全验证。
- [ ] code RX、data/bss RW、stack/guard 正确。
- [ ] exec_path success 保留 PID/fd，failure 保留 old image。
- [ ] checksum 不被误称为安全认证。

### persistence

- [ ] Boot1 创建/写/sync，Boot2 同 image 读取相同文件。
- [ ] `/bin/hello` 从 disk file 真正执行。
- [ ] host dumpfs 与 guest lookup/read 结果一致。

### 理解验收

不看正文回答：

1. fd、OpenFile、inode 为什么要分三层？
2. 为什么 fork/dup2 共享 offset，而两次独立 open 不共享？
3. 为什么 file read 和 readdir 都要 copyout 成功后才推进 offset？
4. `fsync(fd)` 为什么当前仍然是 global sync？
5. NEX1 为什么不是 ELF？checksum 为什么不是信任机制？
6. `exec_path` 如何复用第 24 课 candidate/commit？
7. 两次启动测试最容易被哪个“重新 mkfs”错误做成假阳性？

## 下一阶段为什么自然出现

现在文件可以跨重启保存，但 Stage 7 自己已经明确承认一个缺口：

```text
一个 create/write/mkdir
→ 会修改多个 home blocks
→ crash 可能发生在它们只写了一部分的时候
```

所以“能持久化”还不等于“崩溃后文件系统仍一致”。

下一阶段从 [第 44 课：一次写到一半断掉，会发生什么](44-crash-consistency.md) 开始，先**故意制造**不一致，再做 fsck、journal 和 recovery。