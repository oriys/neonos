# neonos：Rust + RISC-V 操作系统实验课

通过起始代码、分步任务和自动测试，自己实现操作系统的关键机制。

## 开始第一个 lab

```sh
python3 tools/lab.py doctor
python3 tools/lab.py start lab1
cd ../neonos-labs/lab1
cat LAB.md
make grade PART=console
```

第一次测试失败是预期：需要你补齐 TODO。每完成一部分，再运行测试并填写 answers.md。作业有独立 Git 历史，不覆盖主仓库的参考内核。

目前已开放 Lab 1～3，覆盖第 01～15 课：内核基础、用户程序与系统调用、CPU 调度。后续内存、并发、文件系统与恢复 lab 尚在规划中。

- [Lab 入口、评分与提交说明](labs/README.md)
- [完整课程与阅读路线](docs/course/README.md)
- [参考内核运行说明](docs/course/implementation.md)
- [Lab 发布验收](labs/VERIFICATION.md)

已有学生目录时，不要重复 start；直接进入该目录继续。完整作业自动测试使用 make grade，本地打包使用 make submit。
