# 第 02 课：做自己的打印工具

状态：待开始。前置：[第 01 课](01-boot.md) 验收完成。建议拆成 A、B、C 三次，每次 45～60 分钟。

## 为什么下一步是“打印工具”

第 01 课已经发现一个问题：`rust_main` 直接知道 UART 地址，还亲自遍历每个字节。

如果以后 panic、trap、调度器、页分配器都想打印信息，复制这段底层代码会越来越难维护。因此本课只解决一条自然的演进链：

```text
直接写 UART
  ↓
单字节函数
  ↓
字符串函数
  ↓
Rust 格式化接口
  ↓
print!/println! 宏
```

最终希望主流程可以写：

```rust
println!("count={} addr={:#x}", 42, 0x8020_0000usize);
```

本课是后续 OS 实验需要的 Rust 预备课，不要求先掌握完整 trait 和宏系统。

---

## A 次：先把“设备细节”藏进模块

### 本次只引入 4 个概念

| 概念 | 先这样理解 |
| --- | --- |
| `mod` | 把一组相关代码放到一个模块里 |
| `u8` | 一个 8 位无符号整数；这里承载一个输出字节 |
| `&str` | 借用一段 UTF-8 字符串数据，不复制整份字符串 |
| `as_bytes()` | 把字符串按底层字节序列观察 |

### 先读现有代码，不修改

打开 [src/main.rs](../../src/main.rs)，找到：

```text
UART
write_volatile
for byte in b"Hello kernel\n"
```

把它们分别标成：

```text
写到哪里      → UART
怎样真的写     → write_volatile
按什么顺序写   → for 逐字节遍历
```

先回答：如果未来 UART 地址改了，你希望改一个地方还是所有打印调用点？

### 第一步：提取单字节输出

创建 `src/console.rs`，先只迁移设备地址和单字节写入，目标接口：

```rust
pub fn put_byte(byte: u8)
```

要求：

- `UART` 常量不再留在 `rust_main`。
- `write_volatile` 只留在 console 底层。
- `unsafe` 旁边写清楚安全依据：这个固定地址来自当前 QEMU `virt` 实验配置，写入具有设备副作用。

改完立刻编译和运行。不要一次把整课代码都写完再调试。

### 第二步：从一个字节到字符串

再增加：

```rust
pub fn write_str(text: &str)
```

内部逻辑保持简单：

```text
&str
 ↓ as_bytes()
字节切片
 ↓ 逐个 byte
put_byte(byte)
```

让 `rust_main` 只写：

```rust
console::write_str("Hello kernel\n");
```

这里先接受一个重要边界：**当前输出函数发送的是 UTF-8 字符串最终编码后的字节；UART 本身并不知道“字符”是什么。** 第一轮练习继续主要使用 ASCII，避免一开始把多字节显示问题混进设备输出。

### 关于当前 UART 实现的教学边界

当前仓库直接向 QEMU `virt` 的 UART 数据寄存器写字节，没有实现完整 16550 状态轮询、FIFO 配置和并发保护。本阶段只把它当作已知实验环境中的最小启动输出路径。

因此本课能得出的结论是：

> 当前 QEMU 配置下，neonos 已有一条可用于早期诊断的最小串口输出路径。

不能把它扩展成“已经实现了通用 UART 驱动”。后面接收输入和更完整设备行为时会重新核对设备寄存器与状态。

### A 次验收

- [ ] `rust_main` 不再出现 UART 设备地址。
- [ ] `console::put_byte` 可以输出单字节。
- [ ] `console::write_str` 可以保持原 `Hello kernel` 输出。
- [ ] 连续输出三行，顺序正确。
- [ ] `./tests/boot.sh` 仍通过。
- [ ] 能画出 `&str → bytes → put_byte → UART`。

---

## B 次：让 Rust 帮我们格式化数字

### 为什么不能直接把 `{}` 发给 UART

下面这段：

```rust
"count={}"
```

本身只是一段包含 `{}` 字节的字符串。把它直接交给 `write_str`，UART 不会自己把 `{}` 变成数字。

所以要增加一个新层次：

```text
格式模板 + 参数
  ↓ core::fmt
最终文本片段
  ↓ Console::write_str
字节
  ↓ UART
```

### 本次只学习一个 trait

阅读 [core::fmt::Write](https://doc.rust-lang.org/core/fmt/trait.Write.html) 的 `write_str` 和 `write_fmt` 即可。

先这样理解 trait：

> `core::fmt` 不需要知道 UART 是什么；只要某个类型承诺“我会接收字符串片段”，格式化系统就可以把结果交给它。

在 `console.rs` 定义一个不需要保存字段的 `Console` 类型，并为它实现 `core::fmt::Write`。

为了避免“同名函数到底调用谁”的困惑，建议把 A 次的自由函数命名得更明确，例如：

```rust
pub fn write_text(text: &str)
```

而 trait 方法仍然叫：

```rust
fn write_str(&mut self, s: &str) -> core::fmt::Result
```

trait 方法内部只复用 `write_text(s)`，然后返回 `Ok(())`。

### 先不用自定义宏

先引入 `core::fmt::Write` trait，并直接验证：

```rust
let mut console = Console;
write!(console, "count={}", 42);
write!(console, " addr={:#x}\n", 0x8020_0000usize);
```

预期：

```text
count=42 addr=0x80200000
```

如果这里失败，先修 trait 和格式化，不要同时开始写 `println!` 宏。

### 再封装 `_print`

增加：

```text
_print(args: core::fmt::Arguments<'_>)
```

它创建局部 `Console`，再调用 `write_fmt(args)`。

打印路径暂时不使用：

- `String`
- `Vec`
- 堆分配
- 全局可变 writer

格式化错误也不要 `unwrap()` 再制造一次 panic。当前后端实际上不会主动返回格式错误，可以忽略 `fmt::Result` 或用最底层输出固定诊断标记；重点是**panic 打印路径不能轻易递归 panic**。

### B 次验收

- [ ] 能解释 `{}` 不是 UART 功能，而是 `core::fmt` 做的格式化。
- [ ] `count=42` 正确。
- [ ] `addr=0x80200000` 正确。
- [ ] 格式化过程没有引入堆分配。
- [ ] 能解释 trait 在这里把“格式化系统”和“UART 设备实现”解耦。

---

## C 次：最后才做 `print!` 和 `println!`

### 为什么还需要宏

现在如果每次都写：

```rust
_print(format_args!(...))
```

调用仍然很啰嗦。宏只负责把调用语法变短，不负责真正发送字节。

先实现 `print!`：

```text
调用参数
 ↓ format_args!
core::fmt::Arguments
 ↓ $crate::console::_print
格式化并输出
```

再实现 `println!`。至少覆盖：

```rust
println!();
println!("Hello kernel");
println!("count={}", 42);
```

一个常见做法是让带参数的 `println!` 复用 `print!`，用嵌套 `format_args!` 追加换行；不要只做字符串字面量拼接，否则表达式参数会失效。

`$crate` 先理解为“从当前 crate 根找到真正的实现”，避免宏在不同模块展开时依赖调用位置的相对路径。宏系统的完整规则不属于本课。

### 最终验收调用

```rust
println!("Hello kernel");
println!("count={}", 42);
println!("addr={:#x}", 0x8020_0000usize);
println!();
print!("left");
println!(" right");
```

预期依次得到：

```text
Hello kernel
count=42
addr=0x80200000

left right
```

### C 次理解验收

不看正文解释：

1. `println!`、`_print`、`Console::write_str`、`put_byte` 各负责哪一层？
2. `write_volatile` 为什么应该留在最底层，而不是散落在每个调用点？
3. `write_volatile` 为什么不等于原子操作，也不等于线程同步？
4. 格式化一个整数为什么不要求先创建 `String`？
5. 当前输出模块为什么仍不能称为“完整 UART 驱动”？

## 常见问题

| 现象 | 先检查 |
| --- | --- |
| 找不到 `write_fmt` | `core::fmt::Write` trait 是否在作用域 |
| `{}` 原样出现在终端 | 是否绕过 `core::fmt`，把模板直接发给 UART |
| 自定义宏找不到实现 | `$crate` 路径、模块位置和 `_print` 可见性是否一致 |
| `println!()` 无法编译 | 是否单独覆盖无参数分支 |
| panic 时无限重复输出 | 打印实现里是否有 `unwrap`、`panic!` 或递归错误路径 |

## 这一课结束后，系统多了什么能力？

现在其他内核模块不需要知道 UART 地址，就能输出字符串、整数和地址。

这马上解决下一课的问题：如果内核自己 `panic!`，我们终于有能力让错误在停机前“说出自己为什么失败”。

下一课：[让错误说话](03-panic.md) · [阶段安排](stage-01.md)。

完成三次实验后，把真实输出、一个失败案例和自己的分层图写入 [进度记录](progress.md)。