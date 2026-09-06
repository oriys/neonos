#!/bin/sh
# 第 06 课 checkpoint：验证 Program 与 Process 分离，以及最初三态模型的核心语义仍然成立。
set -eu

kernel=target/riscv64gc-unknown-none-elf/debug/neonos
log=$(mktemp)
pid=

cleanup() {
    if [ -n "$pid" ]; then
        kill "$pid" 2>/dev/null || true
    fi
    rm -f "$log"
}
trap cleanup EXIT INT TERM

fail() {
    echo "lesson-06 checkpoint failed: $1" >&2
    cat "$log" >&2 || true
    exit 1
}

# 历史 lesson-06 PR 曾禁止 Faulted；累计分支推进到 lesson-10 后，Faulted 已经第一次真正需要。
# 因此这里只继续保护 lesson-06 的原始契约，并防止尚未出现的 Blocked 被提前加入。
state_enum=$(sed -n '/enum ProcessState {/,/^}/p' src/process.rs)
[ -n "$state_enum" ] || fail "ProcessState enum not found"
if printf '%s\n' "$state_enum" | grep -Fq 'Blocked'; then
    fail "ProcessState contains Blocked before any blocking primitive exists"
fi
printf '%s\n' "$state_enum" | grep -Fq 'Ready' || fail "Ready state missing"
printf '%s\n' "$state_enum" | grep -Fq 'Running' || fail "Running state missing"
printf '%s\n' "$state_enum" | grep -Fq 'Exited' || fail "Exited state missing"

cargo build

qemu-system-riscv64 \
    -machine virt \
    -bios default \
    -kernel "$kernel" \
    -nographic \
    -smp 1 >"$log" 2>&1 &
pid=$!

attempt=0
while [ "$attempt" -lt 50 ]; do
    if grep -Fq '[model] process model ok' "$log"; then
        break
    fi

    if ! kill -0 "$pid" 2>/dev/null; then
        wait "$pid" || true
        fail "QEMU exited before the process model completed"
    fi

    attempt=$((attempt + 1))
    sleep 0.1
done

grep -Eq '^\[model\] program=hello entry=0x[0-9a-fA-F]+ code=\[0x[0-9a-fA-F]+, 0x[0-9a-fA-F]+\)' "$log" \
    || fail "Program is not backed by visible linked entry/range symbols"
grep -Fq '[model] pid=1 Ready' "$log" || fail "pid=1 did not start Ready"
grep -Fq '[model] pid=1 Running' "$log" || fail "pid=1 did not transition Ready -> Running"
grep -Fq '[model] pid=1 Exited(0)' "$log" || fail "pid=1 did not transition Running -> Exited(0)"
grep -Fq '[model] pid=1 reject Exited -> Running kept=Exited(0)' "$log" \
    || fail "invalid Exited -> Running transition was not rejected atomically"
grep -Fq '[model] pid=2 Ready' "$log" || fail "second process instance did not receive pid=2"
grep -Fq '[model] shared_program=true' "$log" || fail "two processes do not reuse the same Program description"
grep -Fq '[model] pid=2 Running' "$log" || fail "pid=2 did not run independently"
grep -Fq '[model] pid=2 Exited(0)' "$log" || fail "pid=2 did not exit independently"
grep -Fq '[model] process model ok' "$log" || fail "process model completion marker missing"

if grep -Fq '[trap]' "$log"; then
    fail "lesson 06 model unexpectedly triggered a CPU trap"
fi
if grep -Fq '[panic]' "$log"; then
    fail "lesson 06 model panicked"
fi

kill -0 "$pid" 2>/dev/null || fail "kernel did not remain alive after the model experiment"

echo "lesson-06 checkpoint passed"
