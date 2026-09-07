# Lab 发布验收

参考内核快照：4085c7d6cdc4a1e3289d70a9db7d9bbc04b5bf96。

本版在独立临时目录验证，而非在参考源码目录直接运行测试：

| 检查 | Lab 1 | Lab 2 | Lab 3 |
| --- | --- | --- | --- |
| 学生起始代码 cargo build | 通过 | 通过 | 通过 |
| 参考版 make grade | 100/100 | 100/100 | 100/100 |
| 未填写 TODO 的学生版 | 0/100 | 0/100 | 0/100 |

此外验证：

- 将学生 tests/lesson-02.sh 改成直接 exit 0，不会让未实现的 console 得分。
- 只补回学生 console 实现后，该部分得到 25/25，不标记整个 lab 完成。
- 已有作业目录拒绝覆盖；起始仓库创建后 Git 状态干净。
- 作业路径包含空格时，生成和打包正常。
- 修改代码后旧评分不会被作为当前评分放入提交包。
- 提交包不包含测试、编译缓存、参考实现的完整源码树或 Git 历史。
- 超时评分命令返回失败并清理其进程组。
- 学生作业放在课程 Cargo 项目之外，评分构建使用独立临时目录，避免父级 .cargo 配置叠加。

六个工具回归测试可运行 `python3 tests/test_lab_runner.py`；CI 通过 `tests/labs.sh` 检查，浅克隆按需取得固定参考 Git 对象。

重新核验完整参考版时，给每个 lab 一个全新的独立目录：

```sh
python3 tools/lab.py start lab1 --reference --dest /tmp/neonos-lab1-check
python3 tools/lab.py grade --workspace /tmp/neonos-lab1-check
```

lab2/lab3 同理。去掉 --reference 即可核验学生版的预期失败。不要在学生实际作业目录中还原参考实现。
