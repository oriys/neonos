# Lab 2：从用户程序回到可信内核

阅读第 06～10 课。建议先完成 Lab 1；这份起始工程已提供可工作的前置代码，你只需要实现本 lab 的 TODO。

允许修改：`src/process.rs`、`src/syscall.rs`、`src/user.rs`、`src/user10.rs`。完整用户栈、trap 栈、TrapFrame 与 KernelContext 汇编桥已提供。起始工程可编译，默认运行会因状态转换未实现而失败，这是预期。

## Part A：进程状态，20 分

实现 `Process::transition(next)`。允许 Ready→Running、Running→Exited、Running→Faulted；拒绝其他转换。非法转换返回 InvalidTransition，必须保留原状态。

```sh
make grade PART=process
```

两个进程共享 Program 描述，但 PID 和状态独立。Exited/Faulted 不能重新变 Running。本 lab 不引入 Blocked。

## Part B：系统调用与用户返回，25 分

实现 `syscall::dispatch(number,arg0)` 和 `user::apply_return(frame,result,code_end)`。

| 请求号 | 参数与结果 |
| --- | --- |
| 1 putchar | 0..127 输出一个字节并返回 0；否则 -2 |
| 2 exit | 0..255 返回 Exit(code) outcome；否则返回 -2 |
| 其他 | 返回 -1 |

普通结果写入保存的 a0；已确认的 ecall 恢复 PC 用 checked +4，检查代码边界；返回状态必须保持 U-mode，关闭本阶段不使用的中断位。负数以 RV64 寄存器位模式传递。

```sh
make grade PART=syscall
```

用户样例自己检查错误码、寄存器和栈哨兵，最后才能打印 OK。依赖 Part A。

## Part C：有效 exit 不再返回，25 分

本部分复用你在 A、B 的实现及已提供的内核返回桥，没有新的 TODO。检查有效 exit 与普通 Return 的分流是否正确：exit(256) 返回 -2，exit(7) 让管理者记录 Exited(7)，不能再执行其后的用户指令。

```sh
make grade PART=exit
```

随后连续运行 100 次，管理栈不漂移，终态不能残留 Running。若失败，先检查 dispatch 的 outcome，不要修改已经提供的汇编去绕过状态机。

## Part D：区分用户 fault 与内核故障，30 分

实现 `src/user10.rs` 的 `handle(frame)`。先验证 frame 的可信栈位置、SPP 来源、中断标志和 PC 范围，再区分 ecall 与受控非法指令。允许当前程序代码和已链接 user_api 的范围；管理器通过 ACTIVE 提供当前程序信息。

普通 syscall 使用 apply_return 并返回 ACTION_RESUME_USER；有效 exit 或用户非法指令保存 RunOutcome 到 ACTIVE，返回 ACTION_TERMINATE_PROCESS，由管理者在安全栈上提交终态。非法指令不能按 ecall 的 +4 规则恢复。

错误 syscall 的诊断契约是：`[user error] syscall=请求号 arg=参数 result=结果`。管理器已有最终完成、Faulted 和退出记录。

必须保留独立 `lesson10-kernel-fault` 场景：在用户处理路径中、内核仍持有当前进程时，调用已声明的 `trigger_illegal_instruction()` 触发 S-mode 故障。它应走独立 kernel vector，不能被当成用户 Faulted。建议阅读 HINTS 中关于这个测试钩子的提示。

```sh
make grade PART=fault
```

验收包含未知调用、非法参数、有效退出、用户 fault 后正常程序继续、100 次运行、独立内核 fault 与 panic。依赖 A、B、C。

## 完成标准

自动评分 100/100，解释题完成。当前仍为 Bare 地址模式，只能证明受控故障路径，不代表能安全执行任意恶意代码。
