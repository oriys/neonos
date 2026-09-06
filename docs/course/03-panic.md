# 第 03 课：让错误说话

状态：待开始。前置：[第 02 课](02-console.md) 验收完成。预计 45～60 分钟。

## 为什么现在学 panic

第 02 课刚得到一个可复用的打印路径。下一件最有价值的事不是继续加功能，而是让内核失败时能留下线索。

今天只解决：

> **Rust 代码主动 `panic!` 时，内核怎样在停住之前告诉我们“发生了什么、在哪里发生”？**

本课不会处理 CPU 非法指令、缺页或定时中断。那些不是 Rust `panic!`，第 05 课开始建立独立的 trap 路径。

## 先观察现在为什么“静默失败”

打开 [src/main.rs](../../src/main.rs) 最下方：

```rust
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    halt()
}
```

先翻译：

```text
Rust 发生 panic
  ↓
进入我们自己的 panic handler
  ↓
完全不读 PanicInfo
  ↓
halt()
  ↓
永远等待
```

所以现在即使内核主动报错，终端也可能只表现成“突然停住”。

再打开 [Cargo.toml](../../Cargo.toml)，确认 dev/release 的 panic 策略是 `abort`。本项目不依赖栈展开来恢复执行；panic handler 的职责是尽量留下诊断，然后进入不可恢复的停机路径。

## 本课只引入两个 Rust 概念

### `PanicInfo`

可以先理解成 panic 处理器收到的一张错误记录。我们只取：

- `message()`：panic 消息；
- `location()`：如果可用，返回源码文件、行、列。

### `Option<T>`

`location()` 可能有值，也可能没有，因此返回 `Option`：

```text
Some(location)  → 有位置
None             → 没有位置
```

本课只需要会用：

```rust
if let Some(location) = info.location() {
    // 使用 location
}
```

不用提前学习 `Option` 的全部方法。

## 先预测输出

我们希望最后形成：

```text
普通启动：
Hello kernel
...

故意 panic：
Hello kernel
[panic] lesson 03 deliberate failure
at src/main.rs:<真实行>:<真实列>
```

panic 后不会回到 `rust_main` 继续执行。

## 分步实验

### 第一步：handler 先能打印固定标记

把 `_info` 改成 `info`，但暂时只增加：

```text
[panic]
```

不要马上主动 panic。先正常 `cargo run`，确认普通启动**没有**出现 `[panic]`。

这一小步验证：修改 handler 没有破坏正常路径。

### 第二步：打印消息和位置

使用第 02 课的打印工具输出：

```text
[panic] <message>
at <file>:<line>:<column>
```

消息可以用 `info.message()` 格式化输出；位置必须处理 `Some/None` 两种情况。没有位置时输出固定文本，例如：

```text
location unavailable
```

panic handler 内不要调用 `unwrap()`，也不要再次 `panic!`。否则“报告错误的代码”本身可能再次报错，最终只留下递归失败。

### 第三步：故意制造一次软件错误

在正常问候语之后临时加入：

```rust
panic!("lesson 03 deliberate failure");
```

并在它后面临时放一个理论上不可达的标记，例如：

```text
SHOULD_NOT_REACH
```

先预测：这个标记会不会出现？

再运行：

```sh
cargo run
```

核对：

- panic 消息与触发文本一致；
- 文件名、行、列对应**本次真实源码**；
- `SHOULD_NOT_REACH` 不出现；
- 报告一次后稳定停住。

源码一改，行号就可能变化。课程不能把某个固定行号写成答案。

## 一个非常重要的测试实验

保持 `panic!` 暂时放在 `Hello kernel` **之后**，另开一次运行：

```sh
./tests/boot.sh
```

观察：它很可能仍然返回成功，因为当前脚本只等待 `Hello kernel`。

这说明：

```text
“测试通过”
只代表测试写出来的条件满足，
不代表程序后面没有发生别的问题。
```

然后把故意 panic 移到 `Hello kernel` **之前**，再预测 `boot.sh` 会怎样。实验后恢复代码。

这一小段不是为了折腾脚本，而是建立后面所有 OS 测试都要用的意识：**先读懂测试判定条件。**

## panic 和 CPU trap 不是同一条路

现在可以画：

```text
Rust 代码主动 panic!
  ↓
#[panic_handler]
  ↓
PanicInfo
```

而非法指令之类的处理器事件以后是：

```text
CPU 检测到异常
  ↓
硬件按照 stvec 进入 trap 汇编入口
  ↓
读取 scause / sepc / stval
```

非法指令不会自动变成一个 `PanicInfo`。反过来，`panic!` 也不是“CPU 自动发现某条非法指令”。

## 当前 panic 打印路径的边界

现在仍是单核、无调度、无并发打印的早期内核，所以直接复用 console 足够教学。

以后有锁、中断和多线程后，panic 可能发生在“原本就持有某个锁”的位置；届时不能无条件假设复杂打印路径永远安全。本课只保证当前阶段的诊断需求，不提前宣称完成了生产级 panic 子系统。

## 常见问题

| 现象 | 先检查 |
| --- | --- |
| 故意 panic 后完全没有 `[panic]` | handler 是否真的调用了第 02 课输出路径 |
| 报重复 panic handler | 是否新增了第二个，而不是修改现有函数 |
| 行号和课程示例不同 | 应以当前源码触发位置为准 |
| `[panic]` 不断重复 | handler 或打印路径是否再次 panic |
| `boot.sh` 通过但人工看到 panic | 脚本只检查 Hello，这是本课故意观察的覆盖缺口 |

## 验收：运行和理解都要通过

### 运行验收

- [ ] 普通启动没有 `[panic]`。
- [ ] 故意 panic 能看到消息与真实位置。
- [ ] panic 后不可达标记不出现。
- [ ] 报告不会无限递归。
- [ ] 移除故意 panic 后恢复正常启动，`./tests/boot.sh` 通过。

### 理解验收

不看正文回答：

1. `panic!` 到 `#[panic_handler]` 的路线是什么？
2. 为什么 `location()` 需要处理“有”和“没有”两种情况？
3. 为什么 panic handler 里不应该随便 `unwrap()`？
4. 为什么 `boot.sh` 在 panic 位于 Hello 之后时仍可能成功？
5. Rust panic 和 CPU 异常最根本的入口差别是什么？

## 小练习

把故意 panic 放进一个新的辅助函数：

```text
rust_main → helper → panic!
```

先猜 `location()` 指向调用 `helper` 的地方，还是实际执行 `panic!` 的地方，再运行验证。完成后移除触发代码。

## 这一课结束后，下一问题自然出现了

现在 Rust 自己主动报错时能留下信息。

但如果 CPU 发现的是非法指令或坏地址，Rust 的 panic handler 根本收不到。要处理 CPU 异常，首先需要知道代码、数据、栈实际位于哪里，并且保证最早期运行环境可控。

所以下一课先画内存地图：[第 04 课：给内核画一张内存地图](04-memory-layout.md)。

最后在 [进度记录](progress.md) 同时记录一次故意失败输出和恢复后的正常输出。