# 第 27 课：用管道连接两个进程

状态：待开始。前置：[第 26 课](26-fork.md) 验收完成。分 A、B、C 三次，每次 45～60 分钟。

## 本课第一次把“进程生命周期”和“阻塞 I/O”放在一起

要实现：

```text
writer process ──bytes──→ pipe ──bytes──→ reader process
```

但真正难的不是环形数组，而是：

- fd 只是一个进程内编号，不是 pipe 本体；
- fork/dup2 会让多个 fd 引用同一个 endpoint；
- read/write 可以只完成一部分；
- empty/full 会 Blocked；
- close 会让 EOF / no-reader 条件突然成立；
- user pointer 错误不能改变已经存在的 pipe 数据。

可以参考 xv6 的 pipe 思路，但 neonos 继续使用自己的“无副作用阻塞 → 重新执行 ecall”模型，不移植其可睡眠 kernel stack。

---

## A 次：先把 fd、endpoint、pipe object 三层分开

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

### 提交前先确认

- `pair_ptr..+8` 是完整可写 user range；
- 当前 fd table 至少有两个空槽；
- pipe pool 有空对象；
- checked 计算无溢出。

即使已经预验证，最终 `copy_to_user` 仍按可失败步骤处理；课程后续有多线程时更不能把“刚才验证过”永久当真。

### 建议提交顺序

```text
reserve pipe object（暂不可见）
→ reserve two fd slots
→ 初始化 read_refs=1, write_refs=1
→ 构造两个 fd entries
→ copy_to_user(pair_ptr, 8 bytes)
→ copyout 成功
→ publish fd entries / pipe live
```

如果实现上需要先把内部 entry 写入表，也必须在 copyout 失败时完整 rollback：

```text
清两个 fd
read_refs/write_refs 回退
释放 pipe slot/generation
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
- [ ] fork 只复制 entries+refs，不复制 ring。
- [ ] exec 保留 entries。
- [ ] dup2 same-fd 和 replacement 引用正确。
- [ ] exit 后所有 endpoint refs 回收。

---

## B 次：部分读写和 Blocked 重试

本阶段协议：

```text
read(fd, buf, len)
write(fd, buf, len)
len <= 256
```

### 共同参数顺序

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

## `read`：先 peek，再 copyout，最后 consume

### pipe 非空

```text
n = min(len, pipe.len)
```

不要先：

```text
read_pos += n
pipe.len -= n
```

再写用户内存。

正确三步：

```text
1. 从 ring PEEK n bytes 到固定 kernel temp buffer
   （不改变 pipe）
2. copy_to_user(buf, n)
3. copyout 全部成功后，才推进 read_pos / pipe.len
```

这样 bad user pointer：

```text
→ -3
→ pipe 字节一个都没被吃掉
```

当前单 hart、kernel syscall 不抢占，所以 peek→copyout→commit 期间没有另一个 Process 修改同一个 pipe。

完成 consume 后，如果 pipe 从 full/近 full 获得了空间，可按 FIFO 规则唤醒一个等待 writer；对方只是 Ready，仍会重新检查。

### pipe 空但还有 writer

为了避免把明显 bad pointer 睡进去，可先验证请求输出范围是合法 U+W user range；然后在同一 scheduler 临界区：

```text
再次确认 pipe empty && write_refs>0
→ 登记 Blocked(PipeRead(pipe_id,...))
→ 保留原 ecall/sepc/参数
→ 回 scheduler
```

被唤醒后重新执行原 read ecall，并再次验证 fd/pointer/pipe state。

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

成功后才：

```text
append n bytes to ring
write_pos/len 更新
```

bad input pointer 不向 pipe 注入任何数据。

写入后可唤醒一个等待 reader。

### pipe 满但还有 reader

先确认 user buffer 当前可读，然后无副作用 Blocked，保留原 ecall。被唤醒后重新验证一切。

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

同理 read 只要读到 `n>0`，就返回 n；用户自己循环处理剩余请求。

这就是为什么第 25 课的“原 ecall retry”只适用于**本次尚未产生副作用**的阻塞路径。

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

---

## C 次：close 会改变等待条件

## EOF

```text
pipe.len > 0
write_refs == 0
```

read 仍先返回剩余数据。

直到：

```text
pipe.len == 0
AND
write_refs == 0
```

read 才返回：

```text
0 = EOF
```

所以“最后一个 writer close”不等于“读者立即丢弃 pipe 里剩余字节”。

最后 writer 消失时，要唤醒**所有**等待 reader，因为 EOF 条件现在对每个等待者都可能成立。

## no reader

```text
read_refs == 0
```

任何 write 返回：

```text
-8
```

本课程不实现 SIGPIPE。

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

## 边界矩阵

至少覆盖：

- bad fd；
- read/write 方向错；
- zero len；
- bad user pointer；
- 跨页 buffer；
- pipe pool full；
- fd table 不够两个空槽；
- partial read/write；
- empty block + wake；
- full block + wake；
- last writer close → drain then EOF；
- last reader close → writer -8；
- fork/dup2/exit 后 ref count。

## 常见问题

| 现象 | 先检查 |
| --- | --- |
| bad read pointer 后数据少了 | 是否 consume 后才 copyout，而不是 peek→copyout→commit |
| bad write pointer 后 ring 多了数据 | 是否修改 ring 前完成 copy_from_user |
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

### 数据正确性

- [ ] partial read/write 不重复、不丢失。
- [ ] read 是 peek→copyout→consume；write 是 copyin→append。
- [ ] >512-byte pattern 完整通过。
- [ ] bad pointer 不改变 ring 状态。

### 阻塞/关闭

- [ ] empty/full Blocked 无副作用，wake 后重试。
- [ ] EOF 在 buffer drain 后正确出现。
- [ ] no-reader 返回 -8。
- [ ] 最后一端关闭唤醒所有相关 waiters。

### 理解验收

不看正文回答：

1. fd、endpoint、pipe object 有什么区别？
2. fork 为什么增加 refs 但不复制 ring？
3. 为什么 read 必须 copyout 成功后再 consume？
4. 为什么 partial success 不能再走“重试原 ecall”？
5. 为什么一个多余 writer fd 会让 reader 永远收不到 EOF？
6. last reader/writer close 为什么还需要 wake blocked 对端？

## 下一课为什么自然出现

现在已经有：

```text
fork + exec + wait + fd + pipe
```

只差让用户能在终端输入一行命令，把这些机制组合起来。

下一课：[第 28 课：终于可以输入命令了](28-shell.md)。把 refs 图、partial I/O pattern 和故意多余 writer 的失败轨迹写进 [进度记录](progress.md)。