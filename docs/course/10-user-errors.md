# 第 10 课：用户程序出错，不等于内核出错

状态：待开始。前置：[第 09 课](09-exit.md) 验收完成。分 A、B 两次，每次 45～60 分钟。

## 这课解决什么问题

现在正常用户程序已经可以：

```text
U-mode
→ syscall
→ 返回
→ exit
→ 内核管理流程继续
```

但“用户程序出错”和“内核自己坏了”不能走同一条结束语义。

本课建立最小故障边界：

```text
用户受控同步异常
→ 只结束当前 Process
→ 管理流程可以继续

内核 S-mode 故障 / panic
→ 这是内核可信边界自身出错
→ 报告并停住
```

当前仍没有页表隔离、没有抢占，所以这里的“用户错误可隔离”只针对课程明确构造的受控 trap。**不能据此宣称已经可以安全运行任意恶意程序。**

---

## A 次：先把 trap 分类做正确

### 第一原则：按“来源 + 原因”分类

不能写：

```text
if current_process exists:
    所有 trap 都算用户错误
```

因为用户 trap 处理期间，内核自己也可能出错，而此时仍然存在 current process。

至少依据保存的：

```text
sstatus.SPP
scause 的 interrupt/exception 标记
scause code
sepc
```

和当前使用的 trap 入口来分类。

### 本阶段明确处理矩阵

| 来源与事件 | 本阶段处理 |
| --- | --- |
| U-mode `ecall`，合法调用 | syscall 分发，返回或 exit |
| U-mode `ecall`，未知号/非法参数 | 返回协议错误码 |
| U-mode 受控非法指令 | 记录 `Faulted`，结束当前实例 |
| S-mode 同步异常 | 内核故障报告，停住 |
| 内核 Rust `panic!` | panic 报告，停住 |
| 未预期中断 | 报告配置/来源并停住排查，不伪装成用户 fault |

这里要特别看出：

```text
未知 syscall
≠ CPU fault
```

用户确实走了合法 `ecall` 协议，只是请求号不认识，所以返回错误更合适。

### 用户 fault 用确定的非法指令制造

复用第 05 课方法，在**用户汇编代码**中放一个带标签的非法编码。

不要用 Rust 空指针访问来制造“看起来像错误”的场景，因为那会把 Rust UB 混进本课。

用户非法指令后记录：

```text
pid / run id
program name
cause
sepc
stval
```

并把 Process 状态设成：

```text
Faulted(FaultInfo { ... })
```

不要对这条未知/非法指令擅自 `sepc += 4` 再返回。

### 复用第 09 课的管理返回路径

trap handler 返回类似：

```text
TerminateFaulted
```

然后：

```text
Rust trap handler 正常返回汇编
→ 汇编恢复 KernelContext
→ run_user 返回管理者
→ 管理者得到 RunOutcome::Faulted
```

不要为 fault 再复制一套完全不同的“跳回内核”实现。

### 做一个强对照

运行顺序：

```text
程序 A：故意用户非法指令
  ↓
管理者记录 A Faulted
  ↓
程序 B：正常 putchar + exit
  ↓
B 完成
```

这个实验真正证明的是：**受控的 A 用户 fault 没有破坏继续运行 B 所需的阶段资源。**

### 再单独做内核故障

内核故障必须放在独立测试模式/独立 QEMU 会话：

```text
S-mode 故意非法指令或 panic
→ 内核报告
→ 停住
→ 不应出现“启动下一个用户程序”的完成标记
```

不要把“内核故障以后还能继续跑”当成本阶段目标。

---

## B 次：把用户 API 和阶段测试整理成真正可重复的实验

### 第一步：统一用户 syscall 包装

创建计划中的：

```text
src/user_api.S
```

把：

```text
putchar
exit
```

封装成用户程序可调用的汇编函数，不让每个样例都手写 `li a7,...; ecall`。

例如概念上：

```text
user_putchar:
    设置 a7=1
    ecall
    ret
```

`exit` 有效时不会返回；参数非法时仍可能返回 -2，所以包装层的返回路径也要定义清楚。

### 用户入口最外层没有普通内核返回地址

用户程序入口不是：

```text
内核 call user_main
```

所以最外层用户逻辑不能用普通 `ret` 指望“回到内核”。

如果设计一个用户启动包装：

```text
user_start
  ↓ call user_main
user_main 返回
  ↓
自动 exit(返回码)
```

那么 `user_main` 的普通 `ret` 只返回到**用户态自己的启动包装**，最后仍由 `exit` 请求内核结束 Process。

### 第二步：建立明确测试选择方式

不要靠：

```text
这次手动把某行注释掉
下次再记得改回来
```

来选择测试。

课程实现时选一种简单、可重复的模式，例如：

```text
明确的编译期测试场景常量 / feature
或固定的阶段测试批次入口
```

要求从命令或代码配置能看出：

```text
现在跑 normal batch
还是 expected kernel fault
```

### 第三步：新增 `tests/user.sh`

新脚本不能复制 `boot.sh` 的缺陷，只看到第一行就成功。

正常批次至少验证：

```text
1. OK 字符 syscall 完成
2. unknown syscall 返回 -1
3. invalid putchar 返回 -2
4. exit(7) → Exited(7)
5. invalid exit 返回 -2 并继续
6. user illegal instruction → Faulted
7. fault 后正常程序仍完成
8. 100 次重复运行完成
9. 最终 stage-complete 标记出现
10. 没有意外 [panic] / kernel-fault / failure marker
```

并设置：

- 总超时；
- QEMU 进程清理；
- 完整日志保留/失败时打印；
- 对关键标记检查顺序与次数，而不是只 grep 一个存在性。

### 第四步：内核故障用独立测试会话

“预期内核故障”脚本成功条件应该是：

```text
看到指定 kernel fault/panic 诊断
AND
没有看到本不应出现的后续成功标记
```

然后测试工具主动结束 QEMU。

不要把“等到超时”本身当成唯一成功证据，因为普通死循环也会超时。

## 第二阶段最终状态图

到这里应该能画出：

```text
                syscall 成功/错误
              ┌──────────────────┐
              ↓                  │
Ready → Running ───────────────→ Running
           │
           ├─ valid exit ──────→ Exited(code)
           │
           └─ user fault ──────→ Faulted(info)
```

`Exited` 和 `Faulted` 都回到内核管理流程，但语义不同。

## 常见问题

| 现象 | 先检查 |
| --- | --- |
| 用户 fault 后内核直接停机 | 来源分类是否把 U-mode trap 当 S-mode fault |
| 内核 fault 也变成 `Faulted(pid)` | 是否仅根据 current process 判断来源 |
| fault 后下一个程序异常 | Process/用户栈/trap 栈/现场是否完整重置 |
| `tests/user.sh` 很快就成功 | 是否只匹配了早期 Hello/OK，而没等最终标记 |
| expected kernel fault 仅靠超时通过 | 是否验证了真正的故障原因和“后续标记不出现” |
| 用户包装 `ret` 跳到奇怪地址 | 最外层用户入口是否缺少用户态启动包装/exit |

## 最终验收

### 运行验收

- [ ] 两次正常 syscall 返回用户。
- [ ] unknown / invalid 参数得到预期错误码。
- [ ] 有效 exit 不返回并得到 `Exited(code)`。
- [ ] 用户非法指令得到 `Faulted(info)`，之后正常程序仍可运行。
- [ ] 内核故障独立报告并停住，不伪装成用户结果。
- [ ] 100 次正常运行没有状态/栈持续漂移。
- [ ] `tests/user.sh` 等待完整最终标记并拒绝意外故障。
- [ ] 原 `tests/boot.sh` 仍通过。

### 理解验收

不看正文回答：

1. 为什么“存在 current process”不能证明 trap 来自用户？
2. unknown syscall 为什么返回错误，而非法指令为什么结束本阶段用户实例？
3. `Exited` 和 `Faulted` 为什么都是终态，但必须分开记录？
4. 为什么用户 fault 和正常 exit 应复用同一个安全管理返回桥？
5. 为什么最外层用户代码不能靠普通 `ret` 回内核？
6. 一个可靠阶段测试为什么既要检查成功标记，也要拒绝意外失败标记？
7. 当前还没有页表，为什么不能声称“任意用户 bug 都不会影响内核”？

## 下一阶段为什么自然出现

第二阶段结束后，我们终于有一个受限但完整的单用户执行闭环：

```text
内核创建运行实例
→ 进入 U-mode
→ syscall
→ 返回用户
→ exit 或 fault
→ 内核管理流程继续
```

但如果用户程序不主动 `exit`，也不发生 fault，它可以一直占用 CPU。

所以第三阶段的问题变成：

> **多个可运行任务存在时，谁先运行？正在运行的程序什么时候把 CPU 交出来？**

完成 [第二阶段总验收](stage-02.md) 并填写 [进度记录](progress.md)，然后进入 [第 11 课：谁先运行，怎么算更好](11-scheduling.md)。