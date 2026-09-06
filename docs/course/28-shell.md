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

# A 次：UART 输入、kernel RX ring 和 idle 唤醒

## 1. 先核对当前 QEMU UART，不凭记忆写寄存器

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

如果 DTB/平台配置声明不同 `reg-shift` 或 register width，就按实际配置计算。

## 2. 把第 02 课最小 UART 输出升级成有状态访问

第 02 课为了启动最小化，直接写 THR。

现在可以让：

```text
put_byte
→ 读 LSR
→ 等待 THRE=1
→ 写 THR
```

仍使用 volatile MMIO；volatile 只表达设备访问副作用，不替代并发锁或 CPU/device ordering。

panic/trap fatal path 不应因此变成一个可能永久复杂阻塞的路径；教学版可以给设备异常增加有界诊断策略。

## 3. `try_read_byte` 不能盲读 RBR

```text
read LSR
if DR == 0:
    return None
else:
    read RBR
    return Some(byte)
```

读取 RBR 会消费一个已接收 byte，所以没有 DR 时不要反复读 offset 0。

---

## 4. 为什么第一版不用 UART external interrupt

真正 UART IRQ 还要同时学习：

```text
PLIC
external interrupt
UART IER/IIR
claim/complete
```

这会一次引入太多层。

本课先复用已经稳定的 SBI timer 做定时轮询：

```text
每到 rx_poll_deadline
→ 读取 UART LSR
→ 最多搬运 N 个 ready bytes
→ 放入 kernel RX ring
```

例如每次最多 drain 32 bytes，避免一次 timer handler 做无界工作。

如果 RX ring 满：

```text
不覆盖未读数据
记录 dropped/overflow count
```

在安全位置统一报告。

## 5. kernel RX ring

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

UART MMIO 和 user buffer 之间永远隔着 kernel-owned ring，不把任意 user pointer 交给设备层。

---

## 6. 扩展 `read(fd=0)`：沿用第 27 课流式 I/O 规则

### 有输入

```text
n = min(request_len, rx_len)
peek n bytes → kernel temp
copy_to_user_exact(buf,n)
copyout 成功 → consume rx ring
return n
```

只验证/写入**实际返回的 n bytes**。

如果 request_len=256，而 ring 只有 5 bytes：

```text
只要求 [buf,buf+5) 可写
```

不要求本次不会使用的尾部 251 bytes 也合法。

### 没输入：不要为了 Blocked 去验证整个 requested buffer

```text
rx_len == 0
AND len > 0
```

此时实际 transfer 是 0，所以只保存整数 syscall 参数：

```text
fd / user VA / len
```

然后：

```text
Blocked(ConsoleRead)
→ 保留原 ecall
→ scheduler
```

不要先 dereference 或验证完整 `[buf,buf+len)`。

原因和 pipe 一样：否则同一个 user pointer 会因为“现在恰好没输入”而被要求验证 256 bytes，但在“现在只有 1 byte 输入”时只需验证 1 byte，产生不一致的 partial-I/O 语义。

被 timer poll 唤醒后重新执行 read：

```text
重新查 fd
→ 重新看 rx_len
→ 得到 actual n
→ 再 exact-copyout [buf,buf+n)
```

`len==0`：有效 ConsoleIn fd 下直接返回 0，不访问 buf。

---

## 7. stdout/stderr write

`fd=1/2`：

```text
actual n = len（第一版同步 console write，不做 partial TX）
copy_from_user(buf,n) → kernel temp
→ console write bytes
→ success return n
```

第一版同步输出，所以没有 console-output Blocked queue；以后做异步 TX 时再重新定义 partial/blocking 语义。

---

## 8. 最容易漏掉的：所有用户都 Blocked 时，谁来轮询 UART？

shell 很正常地会：

```text
read(0)
→ Blocked(ConsoleRead)
```

如果所有用户都 Blocked，而内核又因为 Ready empty 关闭 timer：

```text
永远没有代码检查 UART DR
→ shell 永远醒不过来
```

### 新增 S-mode idle loop

在可信 management/scheduler stack：

```text
while no Ready task:
    判断是否存在未来可唤醒源
    安排下一 rx poll timer
    enable sie.STIE
    enable S-mode global interrupt delivery (sstatus.SIE)
    wfi
    醒来后重新检查 Ready / Blocked / event state
```

这里和第 13～14 课 U-mode 运行时接 S timer 的关键区别：

> CPU 当前就在 S-mode idle，所以同级 S timer 要进入 handler，需要当前 S-mode 的 global interrupt enable 状态允许 delivery，同时 `sie.STIE=1`。

handler 内不主动开放同级 nested interrupt。

`wfi` 只是降低空转，不把“它一定只因我们期望的 timer 返回”当逻辑前提；每次醒来都重新检查条件。

## 9. 统一 timer deadline

已有 RR/MLFQ，又增加 rx polling 时，不能让两个模块互相覆盖 `set_timer`。

统一 timer manager：

```text
next_deadline = min(
    current scheduling slice/allotment/boost deadline（若有 Running user）,
    rx_poll_deadline
)
```

每次 timer trap 分别检查：

- scheduler accounting/preemption；
- UART RX poll。

再预约新的最早 deadline。

### A 次验收

- [ ] UART base/layout 从当前 DTB/平台资料核对。
- [ ] `try_read_byte` 只在 LSR.DR=1 时读取 RBR。
- [ ] RX ring 满不会覆盖未读数据。
- [ ] console read 使用 peek→exact-copyout→consume。
- [ ] 只验证 actual n bytes。
- [ ] 无输入时可以 Blocked，而不会为了睡眠碰完整 user buffer。
- [ ] 所有用户 Blocked 时 S-mode idle 仍能收到 timer、poll UART、唤醒 shell。
- [ ] timer manager 合并 scheduling 和 RX poll deadline。

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

课程建议：

```text
shell/user layer echo
```

kernel `read` 只提供 raw input bytes。

否则字符可能被双重 echo，控制字符语义也会混进 driver。

---

# B 次：把 `fork → exec → wait` 变成真正命令

## parser 保持很小

第一版只匹配：

```text
help
exit
hello
cat
hello | cat
```

可以在 user assembly 中写简单 byte-compare helper。

如果字符串处理开始压过本课主题，不要让用户代码偷调 kernel Rust helper；独立 no_std user runtime 可以以后再做。

## built-in

### `help`

shell 自己通过 fd1 `write` 输出支持列表。

### `exit`

shell 调 user `exit`；KernelManager 回收会话并输出测试完成标记。

## external command：`hello`

```text
shell fork
  ├─ child: exec(HELLO_ID)
  │          ↓ failure
  │          write error to fd2
  │          exit(nonzero)
  │
  └─ parent: wait(child_pid,&status)
             ↓
             再显示 prompt
```

child exec 失败后必须显式 exit；如果直接返回 shell 主循环，会出现两个 shell 同时读 stdin。

## `cat`

```text
loop:
    n = read(0,buf,...)
    if n > 0:
        循环 write(1,...) 直到这 n bytes 全写完
    if n == 0:
        exit(0)
    if n < 0:
        报错并 exit
```

pipe write 可能 partial，所以 cat 不能假设一次 write 就消费全部 n bytes。

### console `cat` 的已知边界

本课不定义 Ctrl-D/terminal EOF。

因此直接：

```text
cat
```

从 ConsoleIn 读取会持续等待未来输入，不会自然 EOF。

pipe 中的 cat 才用于验证 EOF。`help` 应明确说明这个边界。

### B 次验收

- [ ] `help/exit/hello/cat` 解析边界明确。
- [ ] unknown command 不创建 child。
- [ ] hello child exec failure 会 exit，不产生第二个 shell。
- [ ] parent wait 后 prompt 重新出现。
- [ ] console cat 能 Blocked/被输入唤醒。
- [ ] cat 对 output partial write 使用循环。

---

# C 次：实现 `hello | cat`

## 1. 解析必须先于资源分配

只允许：

```text
left | right
```

先验证：

- left 非空；
- right 非空；
- 两侧都是支持的 external command；
- 没有第二个 `|`。

解析失败：

```text
不 pipe
不 fork
```

避免 syntax error 同时泄漏资源。

---

## 2. fd 重定向 helper 必须处理 `src == dst`

第 27 课定义：

```text
dup2(src,dst)
```

当 `src==dst` 时是 no-op success。

因此 shell 不能机械写：

```text
dup2(src,dst)
close(src)
```

否则如果 `src==dst`，刚保留的唯一目标 fd 又被关掉。

定义一个 user-side 小 helper 思维：

```text
redirect_and_close_original(src,dst):
    dup2(src,dst)
    if src != dst:
        close(src)
```

另一个不再需要的 pipe endpoint 同样只在它不是保留目标 fd 时关闭。

当前课程 fd0/1/2 默认已占用、`pipe()` 通常从 3..7 分配，所以标准 shell 场景下 `r/w` 不会等于 0/1；但 API 和教材示例仍按一般正确规则写，不能靠这个偶然布局掩盖 bug。

---

## 3. 正常创建顺序

```text
pipe([r,w])

fork left
  child:
    dup2(w,1)
    if w != 1: close(w)
    if r != 1: close(r)
    exec(hello)
    exec fail → fd2 error + exit

fork right
  child:
    dup2(r,0)
    if r != 0: close(r)
    if w != 0: close(w)
    exec(cat)
    exec fail → fd2 error + exit

parent:
  close(r)
  close(w)
  wait(left)
  wait(right)
  prompt
```

因为 `r` 和 `w` 是 pipe 返回的两个不同 fd，上述条件不会把同一 original endpoint 重复关闭。

### parent 为什么也必须 close

即使 parent 从不读写 pipe，如果保留 write endpoint：

```text
write_refs > 0
```

consumer 在 producer 结束后仍看不到 EOF。

如果保留 read endpoint，也会影响 no-reader 条件和 producer failure 行为。

---

## 4. 为什么必须创建两个 child 后再 wait

错误：

```text
fork producer
wait(producer)
再 fork consumer
```

producer 输出 >512 bytes：

```text
pipe full
→ producer Blocked 等 reader
→ parent wait producer
→ consumer 尚不存在
→ deadlock
```

正确：

```text
两个 child 都创建并完成 fd setup
→ parent 关闭自己的 pipe refs
→ 才 wait
```

---

## 5. 第二次 fork 失败的清理

如果 left 成功、right fork 失败：

parent：

```text
close(r)
close(w)
```

left child 已按正确 fd setup 关闭自己的 read endpoint，只保留 stdout write endpoint。

当 parent 关闭最后 read ref：

```text
left 后续 pipe write → -8
```

如果 left 已 Blocked 在 full pipe，它会被 last-reader close 唤醒再得到 -8。

left 程序必须检查 write result 并 exit，不能无限 retry。

parent 仍必须：

```text
wait(left)
```

确保已经成功创建的 child 被 reap，再回 prompt。

---

## 6. 大数据 pipeline 必须测试

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

`shell.sh` 使用独立 QEMU 会话，把确定输入 bytes 送入 serial stdin，并等待明确 prompt/completion marker。

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

另外增加一个 user-copy 边界测试：

```text
ConsoleIn read 请求 len 大
只有少量 bytes ready
requested tail 跨进无效页
```

必须只按 actual n 验证并成功返回 ready bytes。

检查：

- prompt 次数/顺序；
- child completion；
- pipeline output；
- no unexpected panic/kernel fault；
- timeout 时打印 Process states、wait reasons、fd refs、pipe refs、RX ring state。

---

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
2. S-mode idle 接 S timer 和 U-mode 用户运行时接 timer，在 global interrupt enable 上有什么区别？
3. 为什么 UART read 必须先看 LSR.DR？
4. 为什么 ConsoleIn 在没有 bytes 时不应为了 Blocked 验证完整 requested buffer？
5. 为什么 kernel RX ring 和 user line buffer 是两层不同缓存？
6. `fork→dup2→conditional close→exec` 怎样建立 pipeline endpoint？
7. 为什么 `dup2(src,dst)` 后不能无条件 `close(src)`？
8. parent 为什么必须关闭自己的 r/w，即使自己不读不写？
9. 为什么不能先 wait producer 再创建 consumer？
10. second fork 失败后为什么仍必须 wait 已创建的 child？

完成 [第五阶段总验收](stage-05.md) 并更新 [进度记录](progress.md)。下一步进入 [第 29 课：两个加一，为什么可能只加了一次](29-races.md)。