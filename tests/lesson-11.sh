#!/bin/sh
# 第 11 课 checkpoint：host-side 调度模拟必须给出确定的 FCFS / SJF / RR 时间线和指标。
set -eu

python3 ./tests/lesson-11.py

output=$(python3 ./experiments/scheduling.py)

printf '%s\n' "$output" | grep -Fq 'FCFS' || {
    echo 'lesson-11 checkpoint failed: FCFS result missing' >&2
    exit 1
}
printf '%s\n' "$output" | grep -Fq 'SJF' || {
    echo 'lesson-11 checkpoint failed: SJF result missing' >&2
    exit 1
}
printf '%s\n' "$output" | grep -Fq 'RR(q=2)' || {
    echo 'lesson-11 checkpoint failed: RR result missing' >&2
    exit 1
}

# 教材示例的三个平均值必须稳定，避免只打印时间线却算错指标。
printf '%s\n' "$output" | grep -Fq 'avg_response=4.66667' || {
    echo 'lesson-11 checkpoint failed: FCFS average response changed' >&2
    exit 1
}
printf '%s\n' "$output" | grep -Fq 'avg_response=1.33333' || {
    echo 'lesson-11 checkpoint failed: SJF average response changed' >&2
    exit 1
}
printf '%s\n' "$output" | grep -Fq 'avg_response=2' || {
    echo 'lesson-11 checkpoint failed: RR average response changed' >&2
    exit 1
}

printf '%s\n' "$output" | grep -Fq 'cpu_time=9' || {
    echo 'lesson-11 checkpoint failed: CPU conservation marker missing' >&2
    exit 1
}

echo 'lesson-11 checkpoint passed'
