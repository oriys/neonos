# 第二阶段：运行第一个用户程序

状态：课程已安排，学习待开始。入口：[总路线](README.md) · [进度记录](progress.md)。

目标：让一个内嵌程序真正进入 U-mode，通过系统调用请求内核服务、正确返回并最终退出；内核能把受控用户故障和自身故障分开，并在一个用户实例结束后继续管理流程。

阅读 [OSTEP 官方章节](https://pages.cs.wisc.edu/~remzi/OSTEP/) 第 4 章 Processes、第 5 章 Process API、第 6 章 Direct Execution。OSTEP 解释“为什么需要这些机制”，neonos 课程负责把它们落实到 RISC-V/Rust 实验。

## 开始前检查

先完成 [第一阶段总验收](stage-01.md)：

- 可复用打印工具已经工作；
- panic 会报告；
- 内存/栈边界能解释；
- S-mode 受控 trap 可以进入可信汇编入口并报告。

第一阶段的 trap handler 是“报告后停住”的诊断入口。本阶段不要一上来把它改成“所有 trap 都 `sret`”，而是逐步增加用户来源入口、syscall 返回和管理恢复路径。

## 10 次学习安排

每次 45～60 分钟，建议每周 2～3 次，约 4～5 周。第一次 U-mode 和汇编恢复调试可继续拆分，不为了保持次数而压缩实验。

| 次数 | 课程 | 当次问题 | 当次成果 |
| --- | --- | --- | --- |
| 1 | [06 程序与进程](06-process.md) | Program 和一次运行实例为什么不同 | 最小 Program/Process/状态机，非法状态转换可拒绝 |
| 2 | [07A 用户代码与三类栈](07-user-mode.md) | 真正运行用户代码前要准备什么 | 用户代码、用户栈、trap 栈、管理栈边界清楚 |
| 3 | [07B 第一次准备 U-mode](07-user-mode.md) | `sret` 怎样从 S-mode 进入 U-mode | `sepc/sstatus/sscratch/user sp` 全部由内核构造 |
| 4 | [07C 用户 trap 回内核](07-user-mode.md) | 用户 `sp` 不可信时怎样进入内核 | `sscratch` 交换到可信栈，报告证明来源是 U-mode |
| 5 | [08A syscall 分发](08-syscalls.md) | 内核怎样识别并检查请求 | putchar/错误码分发正确，结果写入 TrapFrame |
| 6 | [08B 返回用户](08-syscalls.md) | 怎样从 `ecall` 后一条指令继续 | `sepc`、寄存器、用户栈恢复正确，两次请求成功 |
| 7 | [09A exit 与继续点](09-exit.md) | 用户结束后怎样不再 `sret` 回去 | KernelContext/run_user 成对恢复，管理者拿到退出结果 |
| 8 | [09B 顺序运行与回收](09-exit.md) | 一个实例结束后怎样安全开始下一个 | 100 次运行无栈漂移/状态残留 |
| 9 | [10A 用户故障分类](10-user-errors.md) | 用户 fault 和内核 fault 如何分开 | 用户非法指令只终止实例，内核 fault 独立停机 |
| 10 | [10B 用户 API 与总验收](10-user-errors.md) | 怎样把阶段行为变成可重复测试 | 用户包装统一，`tests/user.sh` 覆盖完整正常/错误路径 |

## 为什么按这个顺序

```text
先有 Program/Process 模型
  ↓
准备用户代码和可信栈
  ↓
只证明真的进入 U-mode 再 trap 回来
  ↓
才实现 syscall 返回
  ↓
再实现“不返回用户”的 exit
  ↓
最后扩展成 fault 终止和完整阶段测试
```

这样每次只改变一条关键控制流，不会同时调试 U-mode、syscall、exit 和 fault。

## 本阶段共同实现边界

保持：

- 单核；
- 同一时刻只有一个正在运行的用户程序；
- 固定容量资源；
- 不使用堆分配；
- 不启动调度 timer；
- 用户程序是短小整数汇编，链接在内核镜像里；
- 没有独立用户 Rust runtime、ELF loader、argv。

多个程序只按顺序运行：A 完全结束后 B 才开始。

### U-mode 不是完整隔离

本阶段没有页表。U-mode 限制特权指令，但不能据此声称用户已经无法访问所有内核物理内存或设备；实际物理访问还受 PMP 等平台配置影响。

因此本阶段只运行课程受控样例，不用恶意内存写入来证明不存在的安全边界。真正的用户地址空间隔离在第 21 课。

### 没有抢占

用户如果进入纯计算死循环：

```text
不 syscall
不 fault
不 exit
```

内核暂时拿不回 CPU。退出 QEMU 后排查即可。第 13～14 课才接 timer/preemption。

## 共同 syscall 协议

这是课程自定义 ABI，不是 Linux syscall，也不是 SBI：

| 请求 | `a7` | 参数 | 结果 |
| --- | ---: | --- | --- |
| `putchar` | 1 | `a0=0..127` ASCII 字节 | 成功 `a0=0`，非法 `-2` |
| `exit` | 2 | `a0=0..255` | 合法不返回；非法 `-2` |
| 未知请求 | 其他 | 忽略 | `a0=-1` |

本阶段系统调用返回约定：

- `a0` 承载结果；
- 其他保存的用户整数寄存器按课程协议恢复；
- 已处理并要返回的 U-mode `ecall` 前移固定 4 字节；
- exit/fault 终止路径不为了返回用户而修改 `sepc`；
- 无字符串指针，避免在尚无页表时引入用户缓冲区访问问题。

## 三种控制流必须能画出来

### 普通 syscall

```text
U-mode ecall
→ 用户 trap entry
→ 可信 trap 栈
→ syscall
→ TrapAction::ResumeUser
→ 恢复现场
→ sret
```

### 正常 exit

```text
U-mode exit ecall
→ trap
→ Exited(code)
→ trap handler 正常返回汇编
→ 恢复 KernelContext
→ run_user 返回管理者
```

### 用户 fault

```text
U-mode 非法指令
→ trap
→ Faulted(info)
→ 同一安全管理返回桥
→ 管理者可启动下一个受控实例
```

S-mode 内核故障不走后两条“用户终止”路径，而是报告停机。

## 计划产物

学习过程中逐步产生：

```text
src/process.rs
src/user.S
src/syscall.rs
src/user_api.S
扩展 src/trap.rs / src/trap.S
tests/user.sh
```

这些文件的存在不代表课程完成；只有真实实验通过并能解释控制流才算掌握。

## 总验收矩阵

| 场景 | 必须观察到 |
| --- | --- |
| 首次 U-mode ecall | `SPP=0`、cause=8、user sp/trap stack 正确 |
| `putchar('O')`, `putchar('K')` | 用户两次都得到 0，并执行 ecall 后代码 |
| syscall 999 | 用户得到 -1，随后还能正常 syscall |
| `putchar(256)` | -2，不输出截断字节 |
| `exit(7)` | `Exited(7)`，用户 exit 后路径不可达，管理流程继续 |
| `exit(256)` | -2，用户继续运行 |
| 用户非法指令 | `Faulted(info)`，随后正常程序能完成 |
| 内核受控故障 | 独立会话中报告并停住，不变成用户 Faulted |
| 正常程序 100 次 | 完成数正确、栈不漂移、状态重置、资源回到基线 |
| `tests/user.sh` | 等完整最终标记、拒绝意外 panic/fault/failure、超时可诊断 |

`./tests/boot.sh` 仍只负责最早 Hello 启动链，不能代替本阶段测试。

## 阶段理解验收

进入第三阶段前，不看课程回答：

1. Program、Process、TrapFrame、KernelContext 各是什么，生命周期有什么不同？
2. `sret` 为什么不是普通 `ret`？
3. 为什么 U-mode trap 要先离开 user sp？
4. `sscratch` 在返回用户前和 trap 后分别装什么？
5. 为什么只有已处理的 `ecall` 明确 `sepc += 4`？
6. 有效 exit 为什么不返回用户？
7. current process 存在为什么不能证明一个 trap 来自用户？
8. 为什么这个阶段仍不能安全运行任意恶意用户代码？
9. 为什么无限循环用户程序此时仍能独占 CPU？

把真实输出、失败场景和自己的三条控制流图写进 [进度记录](progress.md)，通过后进入 [第三阶段 CPU 调度](stage-03.md)。