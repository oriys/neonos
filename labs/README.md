# neonos Labs：自己实现，再用测试验收

借鉴 MIT [6.5840 Raft lab](https://pdos.csail.mit.edu/6.5840/labs/lab-raft1.html) 的起始代码、分阶段任务和测试驱动流程，以及 [6.1810 xv6 lab](https://pdos.csail.mit.edu/6.1810/2024/labs/util.html) 的独立实验版本与 `make grade` 体验。本项目使用自己的 Rust/RISC-V 教学内核，不是 MIT 官方课程，也没有复制 MIT 的作业实现。

## 第一次使用

在课程仓库根目录：

```sh
python3 tools/lab.py doctor
python3 tools/lab.py list
python3 tools/lab.py start lab1
cd ../neonos-labs/lab1
cat LAB.md
cargo build
make grade PART=console
```

起始代码可以编译，但 TODO 对应的测试应失败。先阅读失败日志，再实现一部分，直到对应测试通过。不要在主课程仓库的 `src/` 中做作业。

完成一个小部分后：

```sh
make grade
# 编写 answers.md，记录原理、失败原因和证据。
git add src answers.md
git commit -m 'Complete lab1 console'
make submit
```

`make submit` 只生成本地 ZIP，包含允许修改的文件、解释题及与当前代码匹配的评分记录，不会自动上传。修改代码后旧评分会标为过期，重新 `make grade` 即可。

## 已经可以做的 lab

| Lab | 对应课次 | 学生实现内容 | 已提供的支撑代码 |
| --- | --- | --- | --- |
| lab1 | 01–05 | 格式化输出、panic 诊断、BSS 清零、trap 安装 | 启动入口、UART、TrapFrame/汇编保存、布局检查 |
| lab2 | 06–10 | 进程状态转换、syscall 分发、用户返回、fault 分类 | 用户/内核栈、完整汇编桥、用户样例与运行管理器 |
| lab3 | 11–15 | FCFS/SJF/RR 模型、MLFQ 模型、Ready 入队、timer 启用 | 任务现场恢复、调度循环、SBI 封装、计算与故障样例 |

每个 lab 的前置功能来自同一份已验证参考快照，因此上一个 lab 没做完也不会让下一份起始工程无法生成；建议仍按顺序学习。每个 lab 内的任务存在依赖，任务书会说明先后次序。

## 后续 lab 规划

| Lab | 对应课次 | 主题 | 状态 |
| --- | --- | --- | --- |
| lab4 | 16–23 | 物理页、堆、Sv39、地址隔离、用户复制 | 规划中 |
| lab5 | 24–28 | exec/wait/fork、pipe、shell | 规划中 |
| lab6 | 29–35 | 线程、锁、条件变量、信号量 | 规划中 |
| lab7 | 36–43 | 块设备、缓存、文件系统与文件接口 | 规划中 |
| lab8 | 44–47 | fsck、日志与崩溃恢复 | 规划中 |

后续部分尚无经过验证的学生起始代码和评分器，不能将课程文档中的规划当作已开放 lab。

## 评分与反馈

- `make grade`：当前 lab 的全部自动测试，总分 100。
- `make grade PART=...`：只跑一个部分，例如 lab1 的 console；显示该部分的分数，不代表整份 lab 已通过。
- 分项名称和分值在各自 `LAB.md` 中列出。
- 完整评分保存在 `target/lab-grades/grade.json`，各分项日志在同目录。
- QEMU 运行日志在 `target/lab-grades/build/test-output/`；单个命令超时会终止它启动的进程组。
- 评分使用固定参考版本的测试，加上实验专用的选择适配器；只导入任务书允许修改的学生文件。修改其他文件不会进入评分构建。
- 自动测试通过不等于完全正确。测试是公开的自学工具，不是防作弊考试服务；仍需解释 `answers.md` 中的问题。

本版参考快照：`4085c7d6cdc4a1e3289d70a9db7d9bbc04b5bf96`。工具支持 macOS/Linux，需要保留课程仓库及这个 Git 对象，浅克隆可先执行 `git fetch origin 4085c7d6cdc4a1e3289d70a9db7d9bbc04b5bf96`。不支持只有源码 ZIP 的副本；作业中的 Makefile 引用课程工具的绝对路径，移动课程仓库后请从新位置执行 `python3 tools/lab.py grade --workspace /你的作业路径`。

## 卡住时如何求助

先提供：你在做哪个 part、预测结果、实际失败日志、已经尝试过的改动。可以要求“只给提示，不直接补完 TODO”。每个 lab 的 `HINTS.md` 按层级提供提示，建议先独立思考再展开下一层。

参考实现仍保留在主课程仓库。只有主动核对时才创建：

```sh
python3 tools/lab.py start lab1 --reference --dest /tmp/neonos-lab1-reference
```

此命令生成完整解答目录，不会把答案写入学生作业；已有目录始终拒绝覆盖。
