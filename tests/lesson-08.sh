#!/bin/sh
# 第 08 课 checkpoint：用户多次 ecall，内核写回 TrapFrame，再恢复并 sret 返回用户。
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
    echo "lesson-08 checkpoint failed: $1" >&2
    cat "$log" >&2 || true
    exit 1
}

# 先验证源码层最关键的两个恢复约束没有被悄悄删掉。
grep -Fq '.checked_add(4)' src/user.rs || fail "recognized ecall no longer advances sepc by checked +4"
grep -Fq 'frame.x[10] = result as usize' src/user.rs || fail "syscall result is not written back to saved a0"

# 这里只匹配真正的汇编指令行，不让注释中的 `sret` / `csrrw` 字样产生假阳性。
restore_t0=$(grep -n -E '^[[:space:]]*ld t0, \{x5\}\(sp\)[[:space:]]*$' src/trap.S | tail -n 1 | cut -d: -f1 || true)
restore_stack=$(grep -n -E '^[[:space:]]*csrrw sp, sscratch, sp[[:space:]]*$' src/trap.S | tail -n 1 | cut -d: -f1 || true)
resume=$(grep -n -E '^[[:space:]]*sret[[:space:]]*$' src/trap.S | tail -n 1 | cut -d: -f1 || true)
[ -n "$restore_t0" ] || fail "user t0 restore is missing"
[ -n "$restore_stack" ] || fail "final user/trap stack swap is missing"
[ -n "$resume" ] || fail "user sret resume is missing"
[ "$restore_t0" -lt "$restore_stack" ] || fail "t0 is restored after abandoning the trusted trap stack"
[ "$restore_stack" -lt "$resume" ] || fail "sret occurs before restoring user sp"

cargo build --features lesson08-syscalls

qemu-system-riscv64 \
    -machine virt \
    -bios default \
    -kernel "$kernel" \
    -nographic \
    -smp 1 >"$log" 2>&1 &
pid=$!

attempt=0
while [ "$attempt" -lt 100 ]; do
    tr -d '\r' <"$log" >"$clean_log"
    if grep -Fxq 'OK' "$clean_log"; then
        break
    fi

    if ! kill -0 "$pid" 2>/dev/null; then
        wait "$pid" || true
        fail "QEMU exited before the user syscall program printed OK"
    fi

    attempt=$((attempt + 1))
    sleep 0.1
done

tr -d '\r' <"$log" >"$clean_log"

grep -Fq '[syscall setup] pid=3 program=user-syscalls' "$clean_log" \
    || fail "lesson 08 did not create the expected real Process"
grep -Fq '[syscall setup] abi=a7:number,a0:arg0/result' "$clean_log" \
    || fail "syscall ABI setup marker missing"
grep -Fq '[syscall setup] spp=0' "$clean_log" || fail "lesson 08 did not prepare U-mode return state"
grep -Fq '[syscall enter]' "$clean_log" || fail "lesson 08 never entered the user program"

# 负错误码必须作为 signed ABI 结果被用户真正读到；否则 user08 会走 fail path，永远不会输出 OK。
grep -Eq '^\[syscall\] number=999 arg0=0 result=-1 sepc=0x[0-9a-fA-F]+ next=0x[0-9a-fA-F]+$' "$clean_log" \
    || fail "unknown syscall did not return/log -1"
grep -Eq '^\[syscall\] number=1 arg0=256 result=-2 sepc=0x[0-9a-fA-F]+ next=0x[0-9a-fA-F]+$' "$clean_log" \
    || fail "invalid putchar argument did not return/log -2"

unknown_count=$(grep -Fc '[syscall] number=999 arg0=0 result=-1' "$clean_log" || true)
invalid_count=$(grep -Fc '[syscall] number=1 arg0=256 result=-2' "$clean_log" || true)
[ "$unknown_count" -eq 1 ] || fail "unknown ecall executed more than once; sepc may not have advanced"
[ "$invalid_count" -eq 1 ] || fail "invalid-argument ecall executed more than once; sepc may not have advanced"

# `OK` 只有在用户汇编自己检查完 -1/-2、s2/s3 和 stack sentinel 后才可能打印。
grep -Fxq 'OK' "$clean_log" || fail "user did not complete the multi-syscall preservation checks"

if grep -Fq '[panic]' "$clean_log"; then
    fail "kernel panicked while handling a recoverable syscall"
fi

# 第 09 课才有 exit；第 08 课成功后用户仍在 U-mode 的 done loop，QEMU 应保持运行。
sleep 0.2
kill -0 "$pid" 2>/dev/null || fail "kernel/QEMU stopped after syscall round trips"

# 恢复默认构建，保证 feature 只是独立教学 checkpoint。
cargo build

echo "lesson-08 checkpoint passed"
