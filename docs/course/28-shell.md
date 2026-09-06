# 第 28 课：终于可以输入命令了

状态：待开始。前置：[第 27 课](27-pipes.md) 验收完成。分 A、B、C 三次，每次 45～60 分钟；UART/idle 调试卡住时可把 A 再拆开。

## 本阶段的最终目标

到目前为止，所有用户程序都是测试管理器预先安排的。

本课第一次让一个真正运行在 U-mode 的 shell 接收终端输入，并把前面学过的机制组合起来：

```text
UART input
→ fd 0 read
→ user shell parser
→ fork
→ dup2
→ exec
→ pipe
→ wait
```

支持范围故意很小：

```text
built-in: help, exit
external: hello, cat
pipeline: exactly one |
```

第一轮不支持：

- 通用参数/argv；
- 引号/转义；
- `>`/`<` 重定向；
- 后台任务；
- job control；
- Ctrl-D 终端 EOF。

遇到不支持的语法要明确报错，不偷偷按错误规则解析。

---

## A 次：UART 输入、kernel RX ring 和 idle 唤醒

## 先核对当前 QEMU UART，不凭记忆写寄存器

QEMU RISC-V `virt` 使用 NS16550-compatible UART，并通过自动生成的 DTB 描述实际设备地址、interrupt 和寄存器布局信息。

当前 neonos 已经把主 UART base 当作：

```text
0x1000_0000
```

本课开始前仍要从当前 QEMU 生成的 DTB/平台资料核对，而不是把这个地址推广成“所有 RISC-V UART 都在这里”。

对于当前标准 8250/16550 byte-register layout（`reg-shift=0`）常用：

```text
RBR/THR offset 0
LSR     offset 5
LSR.DR   bit 0  → receive data ready
LSR.THRE bit 5  → transmit holding register empty
```

如果 DTB/平台配置声明了不同 `reg-shift` 或 register width，就按实际配置计算，不能硬编码旧假设。

### 把第 02 课的最小 UART 输出补成有状态访问

第 02 课为了启动最小化，直接写 THR。

现在设备课程已经开始，可以让：

```text
put_byte
→ 读 LSR
→ 等待 THRE=1
→ 写 THR
```

仍然使用 volatile MMIO；volatile 只保证设备访问不会被当普通内存优化掉，不代替并发锁或设备 memory-ordering 设计。

不要在 panic/trap fatal path 引入无限复杂协议；如果底层 UART 永远不 ready，教学版可采用有界诊断/明确停机策略，而不是让错误报告路径递归失败。

## `try_read_byte` 不能盲读 RBR

设计：

```text
read LSR
if DR == 0:
    return None
else:
    read RBR
    return Some(byte)
```

读取 RBR 会消费一个已接收字节，所以：

```text
没有 DR 时不要反复读 offset 0
```

### 为什么第一版不用 UART 外部中断

真正 UART IRQ 需要继续学习：

```text
PLIC
external interrupt
UART IER/IIR
claim/complete
```

这会一次引入太多新层。

本课先复用已经稳定的 SBI timer 做**定时轮询**：

```text
每到 rx_poll_deadline
→ 读取 UART LSR
→ 最多搬运 N 个 ready bytes
→ 放入 kernel RX ring
```

例如每次最多 drain 32 bytes，避免一次 timer handler 因宿主机积累大量输入而执行无界工作。

如果 RX ring 满：

```text
不覆盖未读数据
记录 dropped/overflow count
```

并在安全位置报告。

## kernel RX ring

固定容量，例如 256 bytes：

```text
rx_read_pos
rx_write_pos
rx_len
```

不变量：

```text
0 <= rx_len <= capacity
```

UART MMIO 和 user buffer 之间永远隔着这个 kernel-owned ring，不让用户指针直接交给设备层。

---

## 扩展 `read(fd=0)`

第 27 课 fd table 已有 `ConsoleIn`。

### 有输入

```text
n = min(request_len, rx_len)
peek n bytes → kernel temp
copy_to_user(buf,n)
copyout 成功 → consume rx ring
return n
```

和 pipe read 一样，bad user pointer 不应该吞掉输入字符。

### 没输入

```text
rx_len == 0
→ 验证当前 user output range
→ Blocked(ConsoleRead)
→ 保留原 ecall
→ scheduler
```

timer poll 发现至少一个新 byte 后：

```text
唤醒等待 ConsoleRead 的任务
```

被唤醒任务重新执行 read ecall，再检查 pointer/ring。

`len==0` 仍按阶段协议：有效 fd 下返回 0，不访问 user pointer。

## 扩展 stdout/stderr write

`fd=1/2`：

```text
copy_from_user(buf,len) → kernel temp
→ console write bytes
→ 成功返回 len
```

第一版同步输出，所以不需要 console-output Blocked queue；以后如做异步 TX 再扩展。

---

## 最容易漏掉的：所有用户都 Blocked 时，谁来轮询 UART？

以前 scheduler 可能在：

```text
Ready empty
```

时认为没有工作并停 timer。

现在 shell 很正常地会：

```text
read(0)
→ Blocked(ConsoleRead)
```

如果所有用户都 Blocked，而内核又关闭 timer：

```text
永远没有代码去看 UART DR
→ shell 永远醒不过来
```

### 新增 S-mode idle loop

在可信 management/scheduler stack 上：

```text
while no Ready task:
    判断是否存在未来可唤醒源
    安排下一 rx poll timer
    enable STIE
    enable S-mode global interrupt delivery (SIE)
    wfi
    被 trap/允许的 wakeup 唤醒
    回 handler/loop 重新检查状态
```

这里和第 13～14 课“U-mode 运行时接 S timer”有一个重要区别：

> **当前 CPU 已经处于 S-mode idle，因此要让 S timer 真正 trap 到 S-mode handler，需要把当前 S-mode 的全局 interrupt enable 状态按规范打开，同时 `sie.STIE=1`。**

进入 timer handler 后，硬件会管理同级中断状态；本课 handler 不主动做嵌套 interrupt。

`wfi` 只用于减少空转，不把“WFI 一定只因目标 timer 返回”当作逻辑前提；每次醒来都重新检查 Ready/Blocked/event state。

### 统一 timer deadline

如果已有 RR/MLFQ deadline，又要 rx polling，不要让两个模块各自随便覆盖 `set_timer`。

timer manager 统一选择：

```text
next_deadline = min(
  current scheduling slice/allotment/boost deadline（若有 Running 用户）,
  rx_poll_deadline
)
```

timer trap 后检查哪些 event 已到期：

- scheduler accounting/preemption；
- UART RX poll；

分别处理，再预约新的最早 deadline。

这样 idle 时只剩 rx poll deadline，用户运行时两种需求共用一个硬件/SBI timer。

### A 次验收

- [ ] 当前 UART base/layout 从 DTB/平台资料核对，而非盲猜。
- [ ] `try_read_byte` 只在 LSR.DR=1 时读 RBR。
- [ ] RX ring 满不会覆盖未读数据。
- [ ] console read 用 peek→copyout→consume。
- [ ] shell read 无输入进入 Blocked。
- [ ] 所有用户 Blocked 时 S-mode idle 仍能收到 timer、poll UART 并唤醒 shell。
- [ ] timer deadline 由一个 manager 合并 scheduler 与 RX poll 需求。

---

## 用户态 line editor

shell 使用固定 128-byte user line buffer。

至少处理：

```text
printable ASCII
CR \r
LF \n
CRLF（只结束一次命令）
Backspace 0x08
Delete 0x7f（也按退格处理）
empty line
```

超长行：

```text
进入 discarding 状态
→ 丢弃直到下一次 line ending
→ 输出 "line too long"
→ 清空状态
```

不要让第 129 字节覆盖 user stack 邻接内容。

### echo 只由一个层次负责

决定：

```text
shell 自己 echo
```

或：

```text
console driver echo
```

本课程建议 shell/user layer echo；kernel `read` 只提供原始输入 bytes。

否则每个字符容易显示两次，控制字符逻辑也混到 kernel driver。

---

## B 次：把 `fork → exec → wait` 变成真正命令

## parser 保持很小

第一版只匹配固定字面命令：

```text
help
exit
hello
cat
hello | cat
```

可以在用户汇编里写简单 byte-compare helper。

如果字符串处理已经变成整课主要难点，不要暗中把 kernel Rust 函数拿来给用户 `call`。可把独立 `no_std` Rust user runtime 作为后续扩展；本课仍保持可解释的受控 user image。

## built-in

### `help`

用户 shell 自己通过 fd1 `write` 输出支持列表。

### `exit`

shell 调用 user `exit`；KernelManager 回收会话并输出测试完成标记。

## external command

例如 `hello`：

```text
shell fork
  ├─ child: exec(HELLO_ID)
  │          ↓ failure
  │          write error to fd2
  │          exit(nonzero)
  │
  └─ parent: wait(child_pid, &status)
             ↓
             再显示 prompt
```

如果 child exec 失败却直接 `ret` 回 shell 逻辑，就会出现“两个 shell 同时读 stdin”的灾难，所以失败分支必须显式 exit。

## `cat`

```text
loop:
    n = read(0,buf,...)
    if n > 0:
        用循环 write(1,...) 直到这 n bytes 全写完
    if n == 0:
        exit(0)
    if n < 0:
        报错并 exit
```

write 可能 partial，不能假设一次就输出 n bytes。

### console `cat` 的已知边界

本课不定义 Ctrl-D/terminal EOF。

所以直接：

```text
cat
```

从 console fd0 读取会持续等待未来输入，不会自然 EOF。

主要用 pipe 中的 cat 验证 EOF；`help` 明确说明裸 `cat` 的这个行为。

### B 次验收

- [ ] `help/exit/hello/cat` 解析有确定边界。
- [ ] unknown command 不创建 child。
- [ ] hello child exec 失败会 exit，不产生第二个 shell。
- [ ] parent wait 后 prompt 重新出现。
- [ ] console cat 能 Blocked/被输入唤醒，partial write 正确。

---

## C 次：实现 `hello | cat`

## 解析必须先于资源分配

只允许一个 `|`：

```text
left | right
```

先验证：

- left 非空；
- right 非空；
- 两侧都是支持的 external command；
- 没有第二个 `|`。

解析失败时：

```text
不 pipe
不 fork
```

避免报语法错误却泄漏资源。

## 正常创建顺序

```text
pipe([r,w])

fork left
  child:
    dup2(w,1)
    close(r)
    close(w)   # 原 fd；fd1 已持有引用
    exec(hello)
    exec fail → fd2 error + exit

fork right
  child:
    dup2(r,0)
    close(r)
    close(w)
    exec(cat)
    exec fail → fd2 error + exit

parent:
  close(r)
  close(w)
  wait(left)
  wait(right)
  prompt
```

如果 `r`/`w` 恰好和目标 fd 相同，`dup2` 的 same-fd 规则要保证后续 close 逻辑不会误关唯一需要的 endpoint；教学测试初期 fd 3+ 分配通常避免这个情况，但 API 仍应定义正确。

## 为什么必须先创建 consumer，再 wait producer

错误：

```text
fork left producer
wait(left)
再 fork right consumer
```

当 producer 输出 >512 bytes：

```text
pipe full
→ producer Blocked 等 reader
→ parent 又 wait producer
→ consumer 还没创建
→ deadlock
```

正确：两个 child 都创建完成、parent 自己的 pipe refs 关闭之后，才 wait。

## 第二次 fork 失败的清理

如果 left 已成功，但 right fork 失败：

parent：

```text
close(r)
close(w)
```

left child 正确的 fd setup 已关闭自己的 read endpoint，只保留 stdout write endpoint。

当 parent 关闭最后 read ref：

```text
left 的后续 write → -8
```

如果 left 已 Blocked 在 full pipe，也会被 last-reader close 唤醒然后得到 -8。

left 用户程序必须检查 write 结果并 exit，不能无限 retry。

parent 仍要：

```text
wait(left)
```

再回 prompt。

## 大数据 pipeline 必须测试

临时换一个 producer 输出 >512 bytes 的确定 pattern，而不只测试短 `hello`。

验证：

```text
producer 因 full Blocked 至少一次
consumer 读出并唤醒
最终完整数据一致
所有 writer refs 关闭后 cat 收到 EOF
两个 child 都 reap
prompt 再出现
```

这能证明 pipeline 不是靠“hello 太短，从来没填满 pipe”侥幸成功。

---

## 自动化 shell 测试

计划：

```text
tests/process.sh
tests/shell.sh
```

`shell.sh` 使用独立 QEMU 会话，把确定输入 bytes 送入当前 serial stdin，并等待明确 prompt/completion marker。

至少脚本化：

```text
empty line
help
unknown
hello
hello | cat
超长 line
重复命令
exit
```

检查：

- prompt 次数/顺序；
- child completion；
- pipeline output；
- 没有意外 panic/kernel fault；
- timeout 时打印 Process states、wait reasons、fd refs、pipe refs、RX ring state。

## 最终资源压力

重复 100 次：

```text
hello
hello | cat
```

结束后和 shell 常驻 baseline 比较：

```text
free frames
Process slots
Zombie count
pipe objects
pipe read/write refs
open fd count
Ready/Blocked queue
```

不能只看“提示符还在”。

## 第五阶段最终理解验收

不看正文回答：

1. 为什么所有用户 Blocked 后 timer 不能直接关闭？
2. S-mode idle 接 S timer 和 U-mode 用户运行时接 timer，在全局 interrupt enable 上有什么区别？
3. 为什么 UART read 必须先看 LSR.DR？
4. 为什么 kernel RX ring 和 user line buffer 是两层不同缓存？
5. `fork→dup2→close→exec` 怎样建立 pipeline endpoint？
6. parent 为什么必须关闭自己的 r/w，即使自己不读不写？
7. 为什么不能先 wait producer 再创建 consumer？
8. second fork 失败后为什么必须 wait 已创建的 child？

完成 [第五阶段总验收](stage-05.md) 并更新 [进度记录](progress.md)。下一步进入 [第 29 课：两个加一，为什么可能只加了一次](29-races.md)。