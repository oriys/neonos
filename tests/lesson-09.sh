#!/bin/sh
# 第 09 课 checkpoint：有效 exit 恢复 KernelContext；非法 exit 仍返回用户；重复 100 次不漂移。
set -eu

kernel=target/riscv64gc-unknown-none-elf/debug/neonos
log=$(mktemp)
clean_log=$(mktemp)
pid=

cleanup() {
    if [ -n "$pid" ]; then
        kill "$pid" 2>/dev/null || true
    fi
    rm -f "$log" "$clean_log"
}
trap cleanup EXIT INT TERM

fail() {
    echo "lesson-09 checkpoint failed: $1" >&2
    cat "$log" >&2 || true
    exit 1
}

# KernelContext/TrapFrame 是两套不同布局；run_user 必须成对 save，terminate 必须 ret 回原 caller。
grep -Fq 'pub struct KernelContext' src/trap.rs || fail "KernelContext type is missing"
grep -Fq 'const _: [(); 128] = [(); KERNEL_CONTEXT_SIZE]' src/trap.rs \
    || fail "KernelContext is not compile-time checked as 128 bytes"
grep -Eq '^[[:space:]]*sd ra, \{kctx_ra\}\(t0\)[[:space:]]*$' src/user.S \
    || fail "run_user does not save kernel ra"
grep -Eq '^[[:space:]]*sd sp, \{kctx_sp\}\(t0\)[[:space:]]*$' src/user.S \
    || fail "run_user does not save kernel sp"
grep -Fq 'terminate_process:' src/trap.S || fail "TerminateProcess assembly branch is missing"
grep -Eq '^[[:space:]]*ret[[:space:]]*$' src/trap.S || fail "terminate path does not return through restored kernel ra"

cargo build --features lesson09-exit

qemu-system-riscv64 \
    -machine virt \
    -bios default \
    -kernel "$kernel" \
    -nographic \
    -smp 1 >"$log" 2>&1 &
pid=$!

attempt=0
while [ "$attempt" -lt 160 ]; do
    tr -d '\r' <"$log" >"$clean_log"
    if grep -Fq '[exit stress] completed=100' "$clean_log"; then
        break
    fi

    if ! kill -0 "$pid" 2>/dev/null; then
        wait "$pid" || true
        fail "QEMU exited before lesson 09 stress completion"
    fi

    attempt=$((attempt + 1))
    sleep 0.1
done

tr -d '\r' <"$log" >"$clean_log"

# exit(256) 是普通错误返回：-2 且 ecall 后继续；只应发生一次。
grep -Eq '^\[exit\] invalid code=256 result=-2 sepc=0x[0-9a-fA-F]+ next=0x[0-9a-fA-F]+$' "$clean_log" \
    || fail "invalid exit did not return -2 and resume"
invalid_count=$(grep -Fc '[exit] invalid code=256 result=-2' "$clean_log" || true)
[ "$invalid_count" -eq 1 ] || fail "invalid exit ecall repeated; sepc may not have advanced"

# exit(7) 是不返回用户的成功路径。
grep -Eq '^\[exit\] code=7 terminate sepc=0x[0-9a-fA-F]+$' "$clean_log" \
    || fail "valid exit did not produce TerminateProcess"
grep -Fq 'kernel resumed: exit=7' "$clean_log" || fail "original Rust manager did not regain control"
grep -Fq 'process state=Exited(7)' "$clean_log" || fail "Process did not commit Exited(7)"
grep -Fq 'kernel_sp_returned=true' "$clean_log" || fail "KernelContext did not restore the manager stack pointer"

# 有效 exit 后的用户失败标记绝不能出现。
if grep -Fq 'BAD_AFTER_EXIT' "$clean_log"; then
    fail "user code after a successful exit was executed"
fi

# 100 次顺序运行不是调度；它用来证明上下文/固定栈可以安全重复复用。
grep -Fq '[exit stress] completed=100' "$clean_log" || fail "100-run lifecycle stress did not complete"
grep -Fq '[exit stress] first_pid=4 last_pid=103' "$clean_log" || fail "stress PIDs did not advance as expected"
grep -Fq '[exit stress] kernel_sp_stable=true' "$clean_log" || fail "kernel management sp drifted across runs"
grep -Fq '[exit stress] trap_frame_stable=true' "$clean_log" || fail "trap frame address drifted across runs"
grep -Fq '[exit stress] no_running=true' "$clean_log" || fail "a stress Process remained Running after exit"

if grep -Fq '[panic]' "$clean_log"; then
    fail "kernel panicked during exit lifecycle handling"
fi

# run_lesson09 完成后在 S-mode 管理路径 stop()，QEMU 应继续存活。
sleep 0.2
kill -0 "$pid" 2>/dev/null || fail "kernel/QEMU stopped unexpectedly after exit stress"

cargo build

echo "lesson-09 checkpoint passed"
