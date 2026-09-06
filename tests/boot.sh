#!/bin/sh
# 上面这行叫 shebang：告诉系统用 `/bin/sh` 来执行这个脚本。

# `set -e`：任意命令失败时，脚本立即退出。
# `set -u`：如果使用了未定义变量，也立即报错退出。
# 合起来可以让测试脚本更严格，避免错误被悄悄忽略。
set -eu

# 保存编译后内核 ELF 的路径。
# 因为 Cargo 默认 target 是 `riscv64gc-unknown-none-elf`，
# 所以 debug 构建结果会出现在这个目录里。
kernel=target/riscv64gc-unknown-none-elf/debug/neonos

# `mktemp` 创建一个临时文件，并把它的路径保存到 `log` 变量。
# 后面 QEMU 的所有输出都会先写进这个文件，方便脚本搜索 `Hello kernel`。
log=$(mktemp)

# 先把 `pid` 初始化为空。
# 稍后 QEMU 在后台启动后，我们会把它的进程 ID 存进这里。
pid=

# 定义一个名叫 `cleanup` 的 shell 函数，用来清理后台进程和临时文件。
cleanup() {
    # `-n "$pid"` 表示：如果 pid 这个字符串不是空的。
    if [ -n "$pid" ]; then
        # 尝试结束 QEMU 进程。
        # `2>/dev/null` 丢弃错误输出；
        # `|| true` 表示即使 kill 失败，也不要让清理流程因此失败。
        kill "$pid" 2>/dev/null || true
    fi

    # `rm -f` 删除临时日志文件。
    # `-f` 表示即使文件已经不存在，也不要报错。
    rm -f "$log"
}

# `trap` 表示：当脚本正常退出（EXIT）、收到 Ctrl-C（INT）或终止信号（TERM）时，
# 自动调用上面的 `cleanup` 函数。
# 这样即使测试失败，也尽量不会留下 QEMU 后台进程和临时文件。
trap cleanup EXIT INT TERM

# 编译内核。
# 因为 `.cargo/config.toml` 已指定 RISC-V target，所以这里不需要额外写 `--target`。
cargo build

# `file "$kernel"` 检查生成文件是什么格式。
# `|` 把上一条命令的输出传给 `grep`。
# `grep -E` 使用扩展正则表达式，`-q` 表示只关心是否匹配，不打印结果。
# 这一步确认：生成物真的是“64 位、小端、RISC-V 的 ELF 可执行文件”。
file "$kernel" | grep -Eq 'ELF 64-bit LSB executable.*RISC-V'

# 启动 QEMU，真正把刚才编译出的内核跑起来。
qemu-system-riscv64 \
    # 使用 QEMU 的通用 RISC-V `virt` 虚拟开发板。
    -machine virt \
    # 使用 QEMU 自带的 OpenSBI 固件。
    -bios default \
    # 把我们的 ELF 作为 kernel 加载。
    -kernel "$kernel" \
    # 不打开图形窗口，把串口输出走终端。
    -nographic \
    # 只启动 1 个 hart（可以先理解成 1 个 CPU 核心）。
    -smp 1 \
    # `>"$log"`：把标准输出写入日志文件；
    # `2>&1`：把标准错误也合并到同一个日志文件；
    # `&`：让 QEMU 在后台运行，这样脚本还能继续执行后面的检查。
    >"$log" 2>&1 &

# `$!` 是 shell 的特殊变量，表示“刚刚启动的后台进程 PID”。
# 这里就是 QEMU 的进程 ID。
pid=$!

# 记录已经检查了多少次日志。
attempt=0

# 最多检查 50 次。
# `-lt` = less than，也就是 attempt 小于 50 时继续循环。
while [ "$attempt" -lt 50 ]; do
    # 在日志里安静地搜索固定字符串 `Hello kernel`。
    # `-F` 表示按普通字符串匹配，不把内容当正则；`-q` 表示不输出匹配内容。
    if grep -Fq 'Hello kernel' "$log"; then
        # 找到了就说明：编译、链接、OpenSBI 启动、_start、Rust、UART 整条链路都成功了。
        # `exit 0` 代表测试成功。
        exit 0
    fi

    # `kill -0 PID` 不是真的杀进程，只用于检查这个 PID 是否还活着。
    # 前面的 `!` 表示取反：如果 QEMU 已经不在运行，就进入这个分支。
    if ! kill -0 "$pid" 2>/dev/null; then
        # `wait "$pid"` 回收后台进程状态。
        # `|| true` 避免 QEMU 的非零退出码让脚本在打印日志前就结束。
        wait "$pid" || true

        # 把 QEMU 日志打印出来，方便看失败原因。
        cat "$log"

        # 以 1 退出，表示测试失败。
        exit 1
    fi

    # shell 的 `$(( ... ))` 是整数算术表达式。
    # 每检查一次，把 attempt 加 1。
    attempt=$((attempt + 1))

    # 等待 0.1 秒再检查下一次，避免疯狂占用 CPU。
    sleep 0.1
done

# 如果循环 50 次都没看到 `Hello kernel`，说明大约 5 秒内启动没有成功。
# 先打印日志帮助排查。
cat "$log"

# 最终以失败状态退出。
exit 1
