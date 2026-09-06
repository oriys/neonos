# 第 03 课：让错误说话

状态：待开始。前置：[第 02 课](02-console.md) 验收完成。预计 45～60 分钟。

## 目标与预习

主动触发一次 panic，让内核报告消息和源代码位置；最后恢复正常启动。

阅读 [PanicInfo 文档](https://doc.rust-lang.org/core/panic/struct.PanicInfo.html) 中的 `message()` 与 `location()`。前者提供消息，后者返回可选的位置，所以要用 `if let Some(...)` 处理。Rust 中的 `Option` 表示“可能有，也可能没有”，先掌握这个例子即可。

## 先解释现有行为（10 分钟）

打开 [src/main.rs](../../src/main.rs) 最下方的 `#[panic_handler]`。当前参数叫 `_info`，函数直接进入 `halt()`，因此主动报错也没有诊断输出。

本课只改现有 panic handler，不增加第二个。查看 [Cargo.toml](../../Cargo.toml) 中 panic 策略，理解本内核不会像普通桌面程序一样恢复到命令行或展开调用栈。

## 分步实验（25 分钟）

1. 把 `_info` 改名为 `info`，在等待前先输出固定标记 `[panic]`。运行普通启动路径，确认没有意外报错。
2. 用 `info.message()` 输出消息。用 `if let Some(location) = info.location()` 输出文件、行、列；缺少位置时输出 `location unavailable`。
3. 在正常问候语之后临时加入 `panic!("lesson 03 deliberate failure");`。
4. 运行 `cargo run`，核对消息、文件名及行号是否对应触发点。截图不是必需，复制关键输出到学习记录即可。
5. 退出 QEMU，删除临时触发语句，再次运行并检查普通启动。

预期形状如下，文件路径和行号以真实运行结果为准：

```text
Hello kernel
[panic] lesson 03 deliberate failure
at src/main.rs:<line>:<column>
```

报告一次后等待属于本课预期，不会自动关闭 QEMU。故意调用 `panic!` 后，后续代码不可达属于预期；清理触发语句后再看编译警告是否仍存在。

## 一个需要分清的概念（5 分钟）

`panic!` 是 Rust 程序主动走错误处理路径；CPU 异常是处理器发现需要交给异常入口处理的事件。非法指令不会自动变成这里的 `PanicInfo`。第五课会建立另一条报告路径。

打印代码本身必须尽量简单：复用第二课不分配堆内存的输出，不在 panic handler 内使用可能再次 panic 的 `unwrap`，也不主动再次调用 `panic!`。

## 常见问题

| 现象 | 先检查 |
| --- | --- |
| 只看到 Hello 就停住 | handler 是否真的打印、宏是否调用到了输出模块 |
| 报重复的 panic handler | 是否新增了一个而没有修改原来的函数 |
| 行号与记录不一样 | 源码编辑会改变行号，应与本次触发语句核对 |
| 启动脚本通过但有 panic | 脚本只检查 Hello，需要另看错误实验输出 |

## 验收与复盘（10 分钟）

- [ ] 故意触发时，消息与位置对应真实触发点。
- [ ] 输出不会无限重复，等待后能退出 QEMU。
- [ ] 删除触发语句后，无意外 panic，启动检查通过。
- [ ] 能解释 `Option` 的两个分支以及 panic 与 CPU 异常的区别。
- [ ] 更新 [进度记录](progress.md)，分别记录错误与正常两条路径。

小练习：把临时 panic 放进新辅助函数中调用，观察报告的位置是函数调用处还是实际 panic 处。实验后移除触发代码。

下一课：[认识内存布局](04-memory-layout.md) · [阶段安排](stage-01.md)。
