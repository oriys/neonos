# 第 02 课：做自己的打印工具

状态：待开始。前置：[第 01 课](01-boot.md) 验收完成。拆成 A、B 两次，每次 45～60 分钟。

## 目标与预习

让主流程通过 `println!("count={} addr={:#x}", 42, 0x8020_0000usize)` 输出数字和地址。

本课是 OSTEP 实验所需的 Rust 预备课。先理解“把输出细节藏进一个模块”，到 B 次再读 [core::fmt::Write](https://doc.rust-lang.org/core/fmt/trait.Write.html) 的接口：实现 `write_str` 后可以使用格式化写入。只按需看方法，不需要读完整个模块。

## 代码导读

打开 [src/main.rs](../../src/main.rs)，找到 `UART`、`write_volatile` 和遍历字节的循环。它们分别回答三个问题：写到哪里、怎样执行写入、按什么顺序发送字符。

本课计划创建 `src/console.rs`，并在 `src/main.rs` 声明 `mod console;`。设备地址和底层写入留在新模块中。

## A 次：先会打印一句话

时间：概念 10 分钟、修改 25 分钟、验证 10 分钟、复盘 5 分钟。

1. 提取 `put_byte(byte: u8)`：先原样迁移已有的单字节写入方式，编译并运行。
2. 提取 `write_str(text: &str)`：通过 `text.as_bytes()` 遍历字节并调用 `put_byte`。
3. 让主流程调用 `console::write_str("Hello kernel\n");`，确认原问候语不变。
4. 连续输出三行，说明字符串、字节切片和单字节分别在哪里出现。

`&str` 可以先理解为“借用已有文字”，不是新分配一份字符串。本课沿用仓库当前 QEMU 串口输出路径，完整 UART 状态轮询在设备课程完善。

`write_volatile` 用于这里具有设备副作用的写入；它不等于线程同步或原子操作。每处 `unsafe` 注释要解释地址为何适用于当前实验机。[Rust volatile 写入文档](https://doc.rust-lang.org/core/ptr/fn.write_volatile.html)

A 次检查：主流程不再知道串口地址；三行文字输出正确；原启动检查通过。

## B 次：先接格式化，最后再做宏

时间：格式化 15 分钟、接入 20 分钟、宏与验证 20 分钟、复盘 5 分钟。

1. 在输出模块定义不含字段的 `Console` 类型，为它实现 `core::fmt::Write`；`write_str` 复用 A 次的函数，完成后返回 `Ok(())`。
2. 引入 `core::fmt::Write`，先用 `write!` 直接验证 `Console`，暂不引入自定义宏。
3. 添加 `pub fn _print(args: core::fmt::Arguments<'_>)`，创建局部 `Console` 并调用 `write_fmt(args)`。
4. 包装 `print!`，把参数通过 `format_args!` 传入 `_print`；模块访问用 `$crate::console::_print`。再实现支持空参数和格式化参数的 `println!`。
5. 用嵌套格式化参数追加换行，不要只允许字面字符串拼接，否则带表达式的调用可能失效。

本课的打印路径不使用 `String`、堆分配或全局可变 writer。处理格式化失败时避免 `unwrap()` 再触发 panic；可回退到底层输出固定错误标记。当前单核、尚未启用定时中断，串口并发互斥留到后续课。

验收调用（在宏已接入后使用）：

```rust
println!("Hello kernel");
println!("count={}", 42);
println!("addr={:#x}", 0x8020_0000usize);
println!();
print!("left");
println!(" right");
```

预期依次看到原问候语、`count=42`、`addr=0x80200000`、一个空行和 `left right`。

## 常见问题

| 现象 | 先检查 |
| --- | --- |
| 找不到 `write_fmt` 方法 | 是否把 `core::fmt::Write` trait 引入作用域 |
| 找不到自定义宏 | 宏的导出方式和调用路径是否一致 |
| 看见花括号而非数字 | 是否绕过格式化，把模板直接当字符串发送 |
| `println!()` 无法编译 | 是否处理了无参数分支 |
| 打印失败后不断 panic | 打印路径里是否有 `unwrap` 或递归打印 |

## 验收与复盘

- [ ] A 次的单字符、字符串输出正常。
- [ ] B 次全部验收调用得到预期输出。
- [ ] `./tests/boot.sh` 通过，并人工确认新增输出。
- [ ] 能解释“格式化文字”和“发送字节”是两件事。
- [ ] 在 [进度记录](progress.md) 留下结果。

小练习：用两次 `print!` 拼出一行启动版本信息。思考为什么每次打印不需要申请一块新的堆内存。

下一课：[让错误说话](03-panic.md) · [阶段安排](stage-01.md)。
