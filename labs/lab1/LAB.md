# Lab 1：让内核能够报告自己的状态

建议用 4～6 次学习完成，每次 45～60 分钟。阅读 `docs/course/00-foundations.md` 与第 01～05 课。目标是建立输出、错误诊断、内存初始化和异常入口；不要求重写整套启动汇编。

## 起始工程

先执行 `cargo build`，确认环境正常。此时 `make grade` 失败是预期：几个关键函数被替换为可编译但不工作的 TODO。搜索：

```sh
rg 'TODO\(lab1' src
```

允许修改：`src/console.rs`、`src/main.rs`、`src/trap.rs`。保留函数签名、输出契约及其他课程的测试配置。其他文件已提供，评分时使用固定版本。

## Part A：格式化输出，25 分

实现 `console::_print(args: fmt::Arguments<'_>)`，把格式化请求接入现有 `Console`。UART 单字节发送、字符串逐字节输出、`fmt::Write` 和打印宏已提供。

要求：支持字符串、十进制整数、十六进制地址、空行和不换行打印；不分配 String，不把示例输出硬编码进 `_print`。

```sh
make grade PART=console
cargo run
```

应依次看到 Hello kernel、count=42、addr=0x80200000、空行、left right。QEMU 退出：Ctrl-A，再按小写 x。

## Part B：panic 诊断，25 分

实现 `src/main.rs` 的 panic handler。第一行是 `[panic] ` 加实际 panic message；有位置信息时输出 `at 文件:行:列`，否则输出 `location unavailable`。打印后进入 `halt()`，不能返回。

```sh
make grade PART=panic
cargo run --features lesson03-panic
```

预期诊断来自真实 PanicInfo；`SHOULD_NOT_REACH` 绝不能出现。依赖 Part A。

## Part C：BSS 清零，25 分

在启动汇编的 TODO 位置恢复真正的逐字节清零操作。循环已经提供起止地址与递增控制。清零区间为 `[sbss, ebss)`，不能清到当前使用的启动栈。

```sh
make grade PART=bss
```

正例先写入非零 probe，再执行清零；Rust 必须读到零。负例显式跳过清零，必须保留 `0x1122334455667788`。不能通过修改 probe 检查或打印零来冒充初始化。依赖 Part A。

## Part D：安装 trap 入口，25 分

实现 `trap::init()`。使用已提供的 `trap_entry_address()`：检查入口 4 字节对齐，禁用 supervisor 中断，设置 stvec Direct 模式，读回并核对，成功后输出 `trap ready`。汇编 TrapFrame 保存和 Rust 故障报告已提供。

```sh
make grade PART=trap
cargo run --features lesson05-illegal-trap
```

受控非法指令应产生 exception cause 2，保存的 sepc 等于触发标签。报告后停住，不猜测非法指令长度并跳过它。依赖 A、C，错误诊断建议先完成 B。

## 完成标准

`make grade` 得到 100/100；在 `answers.md` 中解释启动路线、BSS 对照实验和 trap 的证据。评分会验证行为，也保留了部分课程要求的源码结构检查，因此不要重命名框架里的公开符号。
