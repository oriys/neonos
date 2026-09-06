# 第 01 课：从开机到 Hello kernel

状态：待开始。预计 45～60 分钟。前置：完成或至少通过 [第 00 课](00-foundations.md) 的环境检查；无需提前掌握汇编。见 [第一阶段安排](stage-01.md)。

## 今天只解决一个问题

> **为什么在宿主机执行 `cargo run`，最后会在终端看到 neonos 打出的 `Hello kernel`？**

完成后，你应该不仅“能看到输出”，还能够沿着实际文件指出：谁启动了谁、Rust 为什么没有普通 `main`、栈在哪里准备、字符怎样到达终端，以及程序打印完为什么没有退出。

预习：从 [OSTEP 官网](https://pages.cs.wisc.edu/~remzi/OSTEP/) 打开第 2 章 Introduction，先了解“操作系统负责管理和抽象硬件资源”这个动机即可。不要为了本课通读整章。

## 本课只引入 5 个词

如果第 00 课已经见过，现在只复习用途：

| 名称 | 本课只需要知道 |
| --- | --- |
| QEMU | 模拟 RISC-V `virt` 实验机 |
| OpenSBI | QEMU 中先于 neonos 运行的固件 |
| `_start` | neonos 的最早入口 |
| `sp` / 栈 | `_start` 先给 Rust 函数准备的工作空间 |
| UART | 当前实验机用来把字节送到终端的串口设备 |

先不要学习 trap、页表、SBI 调用或 UART 完整驱动；它们都属于后面的课。

## 先预测：整条启动接力

在打开代码前，先把路线写在纸上或学习记录里：

```text
宿主机 cargo run
  ↓
QEMU 模拟 RISC-V virt
  ↓
OpenSBI
  ↓
neonos::_start
  ↓ 设置 sp
rust_main
  ↓ 写 UART
终端出现 Hello kernel
  ↓
halt() 持续等待
```

现在先做两个预测：

1. `rust_main` 是在 `sp` 设置之前还是之后执行？
2. `Hello kernel` 打完以后，控制流会返回 OpenSBI、退出 QEMU，还是停在 neonos 里？

不要马上找答案，下面用文件验证。

## 对照三个文件找证据（15 分钟）

这一轮**不要从头理解所有注释**，只找能证明启动链的关键行。

### 1. `.cargo/config.toml`：为什么 `cargo run` 会启动 QEMU

打开 [Cargo 配置](../../.cargo/config.toml)，只找：

```text
target = "riscv64gc-unknown-none-elf"
runner = [ ... "qemu-system-riscv64" ... "-kernel" ]
```

先得到两个结论：

- `cargo build` 默认编译的是 RISC-V 裸机目标，而不是宿主机程序。
- `cargo run` 构建完成后，把 ELF 交给 QEMU runner，而不是直接在 macOS/Linux 上执行。

### 2. `linker.ld`：内核入口叫什么

打开 [链接脚本](../../linker.ld)，本课只找：

```text
ENTRY(_start)
. = 0x80200000;
```

先这样理解：最终 ELF 把 `_start` 记为入口，并按当前链接布局从 `0x80200000` 一带安排内核。为什么这个地址适合当前 QEMU/OpenSBI 配置，后续再深入；本课不要把 `0x80200000` 当成所有 RISC-V 内核都必须使用的地址。

### 3. `src/main.rs`：从 `_start` 追到 UART

打开 [内核入口](../../src/main.rs)，按顺序只找下面这些片段：

```text
_start:
la sp, boot_stack_top
call rust_main

rust_main()
for byte in b"Hello kernel\n"
write_volatile(UART, *byte)
halt()
```

把它翻译成普通中文：

```text
_start
  → 把 sp 指到已经预留好的启动栈顶部
  → 调用 rust_main
  → rust_main 把 Hello kernel 的字节一个个写给 UART
  → 打印完成后进入 halt
```

现在回头回答前面的两个预测。

## 为什么这里没有普通 `main`

文件顶部有：

```rust
#![no_std]
#![no_main]
```

本课只需要区分：

- `no_std`：这个 crate 不链接普通 Rust 标准库 `std`；它仍然能使用 `core`。
- `no_main`：不采用普通 Rust 应用程序的 `main` 启动入口。

这两件事不是同一个概念。当前 neonos 自己提供 `_start`，而 `_start` 在满足最基本运行条件后再调用 Rust 函数 `rust_main`。

**检查题：** 如果只写 `no_std` 而仍然依赖普通应用启动环境，和自己提供 `_start` 是不是同一件事？为什么？

## 第一次真实运行（10 分钟）

在项目根目录运行：

```sh
cargo run
```

预期先看到 OpenSBI 的启动信息，之后出现：

```text
Hello kernel
```

随后程序不会自动退出。这不是“卡死”的证据：当前 `rust_main` 最后进入 `halt()`，而 `halt()` 在循环中执行 `wfi`。`wfi` 的精确硬件语义以后再学；这里真正保证控制流不会掉进未知代码的是外层无限循环。

退出 QEMU：先按 `Ctrl-A`，松开，再按 `X`。

如果运行失败，不要先改内核。回到 [第 00 课的环境定位表](00-foundations.md#00j学会记录哪一层坏了)，记录第一条错误属于 Cargo、target、QEMU、链接还是启动路径。

## 动手：只改一个最小东西（10～15 分钟）

现在才修改代码。

在 `rust_main` 的字节字符串中保留原问候语，增加第二行，例如：

```rust
b"Hello kernel\nWelcome to neonos!\n"
```

注意：这是替换循环遍历的字节字符串，不是在 `halt()` 后面追加代码。

修改前先写预测：

```text
我预计终端会多看到：________________
我预计 `\n` 会：____________________
```

再运行：

```sh
cargo run
```

确认实际结果和预测是否一致。

当前练习只使用 ASCII，是为了暂时不把 UTF-8 多字节编码和 UART 字节输出混到第一课。后面输出工具仍会以“最终发送字节”为基础。

## 再跑自动检查：它到底证明了什么

运行：

```sh
./tests/boot.sh
```

它应该成功，因为我们保留了 `Hello kernel`。

但必须明确：当前脚本只在日志中寻找固定标记 `Hello kernel`。因此：

```text
boot.sh 通过
≠ 新增 Welcome 一定出现
≠ 打印以后一定没有 panic
≠ 未来的分页/调度功能正确
```

本课新增的欢迎语仍需要人工观察。理解“一个测试实际验证了什么、没有验证什么”是后面做内核实验的重要习惯。

## 常见问题

| 现象 | 先检查 |
| --- | --- |
| 找不到 Cargo 或 QEMU | 回到第 00 课环境检查，不改内核代码 |
| 提示目标所需的 `core` 不存在 | 是否安装 `riscv64gc-unknown-none-elf` target |
| 能构建，但只有 OpenSBI 输出 | `_start`、链接入口、UART 路径是否仍和当前代码一致 |
| 出现 `Hello kernel` 后不退出 | 当前 `halt()` 永久等待，这是预期行为 |
| 改欢迎语后 `boot.sh` 失败 | 是否把原来的 `Hello kernel` 标记删掉或改掉 |
| `boot.sh` 通过但没看到新欢迎语 | 脚本本来就不检查新行，应重新人工运行观察 |

## 验收：必须同时通过“运行”和“解释”

### 运行验收

- [ ] `cargo run` 能看到 `Hello kernel` 和自己的新增文字。
- [ ] 能按 `Ctrl-A`、`X` 正常退出 QEMU。
- [ ] `./tests/boot.sh` 通过。

### 理解验收

不看课程正文，自己回答：

1. `cargo run` 为什么没有直接执行一个宿主机二进制？
2. `ENTRY(_start)` 和 `call rust_main` 分别发生在哪两个层次？
3. 为什么必须先设置 `sp` 再进入普通 Rust 函数？
4. `Hello kernel` 从字符串到终端，至少经过哪两步？
5. 为什么打印结束后 QEMU 不会自动退出？
6. `boot.sh` 通过具体证明了什么，又没有证明什么？

只要其中有一题说不清，就回到对应的三份文件找证据，而不是背答案。

## 小实验：故意打破一次假设

把新增欢迎语临时放到 `Hello kernel` **前面**，预测输出顺序，再运行验证。然后恢复你希望保留的最终顺序。

这个实验很简单，但要建立一个后续一直使用的习惯：

```text
先预测 → 改一个变量 → 运行 → 比较实际结果 → 再解释
```

最后在 [进度记录](progress.md) 写下真实输出和至少一个还没弄懂的问题。

## 这一课结束后，系统多了什么能力？

严格说，内核能力没有新增：仓库本来就能启动和打印。**新增的是你对现有最小内核的可解释性。**

你现在已经知道输出逻辑直接写死在 `rust_main` 里。下一课自然会遇到一个问题：以后 panic、trap、调度器都想打印，难道每个地方都重新写一遍 UART 循环吗？

所以进入下一课：[做自己的打印工具](02-console.md)。