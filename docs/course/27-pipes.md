# 第 27 课：用管道连接两个进程

状态：待开始。前置：[第 26 课](26-fork.md) 验收完成。分 A、B、C 三次，每次 45～60 分钟。

## 本课第一次把“进程生命周期”和“阻塞 I/O”放在一起

要实现：

```text
writer process ──bytes──→ pipe ──bytes──→ reader process
```

真正难的不是环形数组，而是：

- fd 只是一个进程内编号，不是 pipe 本体；
- fork/dup2 会让多个 fd 引用同一个 endpoint；
- read/write 可以只完成一部分；
- empty/full 会 Blocked；
- close 会让 EOF / no-reader 条件突然成立；
- user pointer 错误不能改变已经存在的 pipe 数据；
- “现在不能传任何字节”与“最终实际会传多少字节”不能混成同一个 pointer-validation 规则。

可以参考 xv6 的 pipe 思路，但 neonos 继续使用自己的“无副作用阻塞 → 重新执行 ecall”模型，不移植其可睡眠 kernel stack。

---

# A 次：先把 fd、endpoint、pipe object 三层分开

## 1. fd 是 Process 内的索引

每个 Process 固定 8 个 fd：

```text
0 stdin
1 stdout
2 stderr
3..7 可分配
```

`fd=3` 只是当前 Process 的表下标。

两个进程都可以有 `fd=3`，却指向不同对象；同一个进程的 `fd=3`、`fd=7` 也可能通过 `dup2` 指向同一个 endpoint。

## 2. fd entry 指向“打开的端点”

第一版 entry 可以表示：

```text
ConsoleIn
ConsoleOut
ConsoleErr
PipeRead(pipe_id, generation)
PipeWrite(pipe_id, generation)
```

generation 防止旧 fd entry 意外命中已经释放并复用的 pipe slot。

## 3. pipe object 才保存共享字节流

全局固定 pipe pool，例如 4 个：

```text
512-byte ring buffer
read_pos
write_pos
len
read_refs
write_refs
reader wait queue
writer wait queue
state/generation
```

环形不变量：

```text
0 <= len <= 512
```

读写位置按容量取模；不要靠“write_pos==read_pos”单独判断空/满，`len` 已明确区分二者。

---

## `pipe(pair_ptr)` 是一个小事务

返回两个 32-bit fd：

```text
pair[0] = read fd
pair[1] = write fd
```

它是第 25 课“小型结构化 copyout”的第二个使用者。

### 提交前先确认资源

- checked `pair_ptr + 8` 不溢出；
- 当前 fd table 至少有两个空槽；
- pipe pool 有空对象。

用户输出范围最终由：

```text
copy_to_user_exact(pair_ptr, 8 bytes)
```

统一做完整 U+W/current-AddressSpace user-RAM 验证。

### 为什么必须 exact copyout

如果 8-byte pair 跨页：

```text
前 4 bytes 可写
后 4 bytes 不可写
```

错误的边写边验证可能得到：

```text
pair[0] 已经出现在用户内存
pair[1] 没写
pipe() 返回 -3
```

这不是一个完整的 syscall 控制结果。

课程要求：

```text
validate all 8 bytes
→ 全部合法以后才真正写 pair[0]/pair[1]
```

失败时用户 pair buffer 保持未提交状态。

### 建议提交顺序

```text
reserve pipe object（暂不可见）
→ reserve two fd slots
→ 初始化 read_refs=1, write_refs=1
→ 构造两个 fd entries
→ copy_to_user_exact(pair_ptr, 8 bytes)
→ exact copyout 成功
→ publish fd entries / pipe live
```

如果 copyout 失败：

```text
清 reservations
read_refs/write_refs 回退
释放 pipe slot/generation
用户 pair buffer 不部分提交
```

用户得到 -3/-4 时不能留下“看不见的 pipe”。

---

## fork / exec / close / exit 的引用规则

### fork

从本课开始，第 26 课的 fork 需要扩展：

```text
复制 parent fd entries
→ 对 PipeRead 增加 read_refs
→ 对 PipeWrite 增加 write_refs
```

**不复制 pipe ring buffer 本体。**

父子看到的是同一个字节流对象。

如果 fork 后续失败，所有已经增加的 endpoint ref 也必须 rollback。

### exec

第 24 课规则正式生效：

```text
exec 保留 fd table
```

否则 shell 以后做：

```text
dup2(pipe_write, 1)
exec(hello)
```

重定向会被 exec 清掉。

### close

```text
close(fd)
```

- fd 不存在 → -7；
- 删除本 Process 的 entry；
- 对应 endpoint ref -= 1；
- 根据“最后一个 reader/writer 是否消失”触发唤醒条件。

### exit/fault

Process 真正结束时：

```text
逐个 close 所有仍打开 fd
```

每个 entry 只释放一次，随后再走 Zombie 流程。

### pipe object 何时能释放

```text
read_refs == 0
AND
write_refs == 0
```

才允许 pipe slot 回收。

由于一个 Blocked read/write 仍拥有发起调用的 fd，正常情况下 waiter 本身会贡献对应 endpoint ref；第六阶段加入“其他线程 close 正在使用的 fd”时会增加额外保护规则。

---

## `dup2(src,dst)` 的提交规则

1. `src` 必须是有效 open fd；
2. `dst` 必须在 fd table 范围内；
3. `src == dst`：直接返回 `dst`，不改任何 ref；
4. 否则先准备一份 source entry 的新引用；
5. 增加 source endpoint ref；
6. 如果 dst 已打开，关闭旧 dst entry 并减少它的 ref/触发相应 close 条件；
7. 把 source copy 安装到 dst；
8. 返回 dst。

先增加 source ref 再替换 dst，使得即使 src/dst 最终指向同一个 pipe object，也不会在中间把对象错误释放。

当前内核 syscall 路径不可抢占，wake 的任务在本次 dup2 完成后才会真正运行，所以替换操作对用户观察保持一个完整内核操作。

### A 次验收

- [ ] fd / endpoint / pipe object 三层能画清楚。
- [ ] `pipe()` 任意失败无 fd/ref/object 泄漏。
- [ ] 跨页 pair 第二页坏时，用户内存不出现“只有一个 fd”的半结果。
- [ ] fork 只复制 entries+refs，不复制 ring。
- [ ] exec 保留 entries。
- [ ] dup2 same-fd 和 replacement 引用正确。
- [ ] exit 后所有 endpoint refs 回收。

---

# B 次：部分读写和 Blocked 重试

本阶段协议：

```text
read(fd, buf, len)
write(fd, buf, len)
len <= 256
```

## 共同参数顺序

先检查：

```text
len <= 256
fd 存在
fd 方向正确
```

`len==0`：

```text
返回 0
不访问 buf
```

即使零长度，也仍要求 fd/方向有效，保持 API 规则确定。

---

## 一个关键规则：pointer validation 只覆盖“这次真正要传输的 n bytes”

pipe 支持 partial I/O。

例如：

```text
read(buf, 256)
```

但当前 pipe 只有：

```text
80 bytes
```

本次实际语义是：

```text
n = 80
```

因此只需要 `buf[0..80)` 可写；不能额外要求 `buf[80..256)` 也合法。

同理 write 当前只有 80 bytes 空间时，只会读取前 80 bytes。

所以课程统一：

> **先根据 pipe 状态算出实际非零 `n`，再验证/复制恰好 `[buf,buf+n)`；不要为了“请求 len 很大”预先触碰本次根本不会传输的尾部。**

这个规则也决定了 empty/full 的 Blocked 路径怎样处理 user pointer。

---

## `read`：先 peek，再 exact copyout，最后 consume

### pipe 非空

```text
n = min(len, pipe.len)
```

正确三步：

```text
1. 从 ring PEEK n bytes 到固定 kernel temp buffer
   （不改变 pipe）
2. copy_to_user_exact(buf, n)
3. exact copyout 全部成功后，才推进 read_pos / pipe.len
```

这样如果 `[buf,buf+n)` 跨页第二页非法：

```text
→ -3
→ pipe 字节一个都没被吃掉
→ 用户 buffer 也不留下本次半份 copyout
```

当前 single-hart、kernel syscall 不抢占且无 user remap，因此 peek→validate/write→consume 期间 pipe 和映射都保持稳定。

完成 consume 后，如果 pipe 获得了空间，可按 FIFO 规则唤醒一个等待 writer；对方只是 Ready，仍会重新检查。

### pipe 空 + 仍有 writer：Block 时不要先验证整个请求 buffer

```text
pipe.len == 0
AND write_refs > 0
```

此刻实际传输：

```text
n = 0
```

因此课程不为了 Blocked 去 dereference/完整验证：

```text
[buf,buf+len)
```

只保存 syscall 的原始整数参数：

```text
fd / user VA / len
```

然后在同一个 scheduler-safe 临界区：

```text
再次确认 pipe empty && write_refs>0
→ 登记 Blocked(PipeRead(pipe identity))
→ 保留原 ecall/sepc/args
→ scheduler
```

被唤醒后重新执行 ecall：

1. 重新验证 fd/generation/direction；
2. 重新检查 EOF/pipe.len；
3. 如果 `n>0`，才验证并 exact-copyout `[buf,buf+n)`。

### 为什么允许 bad pointer 先睡一会儿

因为当前条件下没有数据可以返回。

如果我们在 empty path 预验证全部 `len`，同一个调用会出现奇怪的不一致：

```text
pipe empty  → 因 buf 尾部坏而立即 -3
pipe 有 80B → 只需前 80B，反而成功
```

那等于“地址是否合法”由 pipe 当时有没有数据决定。

课程选择一致的 partial-I/O 语义：**只验证实际要传的字节。**

---

## `write`：先 copyin，再修改 ring

### pipe 有空间且仍有 reader

```text
n = min(len, free_space)
```

先：

```text
copy_from_user(buf, n) → kernel temp
```

全部成功后才：

```text
append n bytes to ring
write_pos/len 更新
```

bad input pointer 不向 pipe 注入任何数据。

写入后可唤醒一个等待 reader。

### pipe full + 仍有 reader：同样不预读整个 buffer

```text
free_space == 0
AND read_refs > 0
```

此刻 `n=0`，所以不要为了睡眠先读取/验证 `[buf,buf+len)`。

在 scheduler-safe 临界区重新确认 full+reader 后：

```text
Blocked(PipeWrite(pipe identity))
→ 保留原 ecall/整数参数
```

被唤醒再重新检查对象和 free space；只有得到 `n>0` 时才 `copy_from_user(buf,n)`。

### no-reader 比 user pointer 优先成为语义结果

如果：

```text
read_refs == 0
```

则本课返回：

```text
-8
```

不读取 user buffer。因为根本不会向 pipe 传任何 byte。

### 已经完成一部分就立刻返回

假设用户要求 256 bytes，但 pipe 只有 80 bytes 空间：

```text
write 80
→ return 80
```

**不要：**

```text
先写 80
→ 再 Blocked
→ wake 后从原 ecall 再写这 80
```

否则会重复数据。

同理 read 只要得到 `n>0` 就返回 n；用户自己循环处理剩余请求。

这就是为什么第 25 课的“原 ecall retry”只适用于**本次还没有产生不可重复副作用**的阻塞路径。

---

## 大于 ring 的完整性实验

生产者写一个总长度 >512 的确定 pattern，例如多个编号 chunk；消费者用不同大小的 read 循环。

最终验证：

```text
总字节数一致
每个位置内容一致 / checksum 一致
无重复
无缺失
```

不能只看“打印出来好像差不多”。

### 增加一个 partial-buffer 权限测试

构造：

```text
len = 256
buf 前 80 bytes 位于合法页
buf 后部跨入 unmapped/只读页
```

- pipe 当前正好有 80 bytes → read 应允许返回 80；
- pipe empty + writer alive → 先 Blocked，不因未使用尾部立即 -3；
- 后续若一次实际要传的 `n` 跨入坏页 → 才返回 -3，pipe 不 consume。

这能证明实现真的按“actual n”而不是“requested len”验证。

---

# C 次：close 会改变等待条件

## EOF

如果：

```text
pipe.len > 0
write_refs == 0
```

read 仍先返回剩余数据。

直到：

```text
pipe.len == 0
AND write_refs == 0
```

read 才返回：

```text
0 = EOF
```

EOF 不传输用户 bytes，所以：

```text
不访问 buf
```

这和 zero-length/no-reader 的“没有实际 byte transfer 就不碰 pointer”规则一致。

最后 writer 消失时，要唤醒**所有**等待 reader，因为 EOF 条件现在对每个等待者都可能成立。

## no reader

```text
read_refs == 0
```

任何非零 write 返回：

```text
-8
```

本课程不实现 SIGPIPE，也不读取 user buffer。

最后 reader 消失时，要唤醒所有 blocked writer，让它们重新执行并看到 `-8`，不能永远睡在“pipe full”。

## 故意制造一个“多余 writer fd”错误

典型：

```text
parent 创建 pipe
fork child
child/parent 某处忘记 close 一个 write endpoint
```

真正 producer 已退出，但：

```text
write_refs 仍 > 0
```

reader 读空后不会 EOF，而是继续 Blocked。

先观察 wait/refs 日志，再修掉多余 fd。这个实验对以后 shell 管道非常重要。

---

## 边界矩阵

至少覆盖：

- bad fd；
- read/write 方向错；
- zero len；
- bad user pointer；
- 跨页 buffer；
- requested len 尾部非法但 actual `n` 前缀合法；
- pipe pool full；
- fd table 不够两个空槽；
- `pipe(pair_ptr)` 跨页第二页坏且无半结构 copyout；
- partial read/write；
- empty block + wake；
- full block + wake；
- last writer close → drain then EOF；
- last reader close → writer -8；
- fork/dup2/exit 后 ref count。

## 常见问题

| 现象 | 先检查 |
| --- | --- |
| bad read pointer 后数据少了 | 是否 consume 后才 copyout，而不是 peek→exact-copyout→consume |
| read 失败后用户 buffer 被改了一半 | 是否缺 validate-all → write 的 exact copyout |
| bad write pointer 后 ring 多了数据 | 是否修改 ring 前完成 `copy_from_user(buf,n)` |
| empty pipe + 大 len 无端 -3 | 是否错误验证了本次不会传输的整个 requested range |
| 内容重复 | partial nonzero 后是否还错误 Block/retry 原 ecall |
| reader 永远等不到 EOF | 是否还有隐藏 write fd/ref |
| blocked writer 在无 reader 后不醒 | last-reader close 是否 wake all writers |
| dup2 后 refs 越来越大 | same-fd/替换时 inc/dec 是否成对 |
| pipe object 提前复用 | generation/ref release 条件是否正确 |

## 最终验收

### fd 生命周期

- [ ] pipe/fork/dup2/close/exit 的 refs 可逐步核对。
- [ ] exec 保留 fd。
- [ ] pipe object 只在两类 refs 都为 0 时释放。
- [ ] `pipe()` 控制结果采用 exact copyout，不出现半个 fd pair。

### 数据正确性

- [ ] partial read/write 不重复、不丢失。
- [ ] read 是 peek→exact copyout→consume；write 是 copyin→append。
- [ ] pointer 只验证本次实际传输的 `n` bytes。
- [ ] >512-byte pattern 完整通过。
- [ ] bad pointer 不改变 ring 状态。

### 阻塞/关闭

- [ ] empty/full Blocked 前没有 user-buffer 副作用，wake 后重试。
- [ ] Blocked 不要求预验证本次不会传输的 requested tail。
- [ ] EOF 在 buffer drain 后正确出现且不访问 buf。
- [ ] no-reader 返回 -8 且不访问 buf。
- [ ] 最后一端关闭唤醒所有相关 waiters。

### 理解验收

不看正文回答：

1. fd、endpoint、pipe object 有什么区别？
2. fork 为什么增加 refs 但不复制 ring？
3. 为什么 read 必须 exact copyout 成功后再 consume？
4. 为什么 partial success 不能再走“重试原 ecall”？
5. 为什么 pointer validation 应按 actual `n`，而不是机械验证 requested `len`？
6. 为什么一个多余 writer fd 会让 reader 永远收不到 EOF？
7. last reader/writer close 为什么还需要 wake blocked 对端？

## 下一课为什么自然出现

现在已经有：

```text
fork + exec + wait + fd + pipe
```

只差让用户能在终端输入一行命令，把这些机制组合起来。

下一课：[第 28 课：终于可以输入命令了](28-shell.md)。把 refs 图、partial-I/O 权限边界和故意多余 writer 的失败轨迹写进 [进度记录](progress.md)。