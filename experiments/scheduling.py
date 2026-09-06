#!/usr/bin/env python3
"""Lesson 11 host-side scheduling simulator.

Models non-preemptive FCFS/SJF and Round Robin with deterministic event ordering:
1. settle the interval that just ended / completed task
2. admit tasks arriving at the current time in input order
3. requeue an expired RR task
4. choose the next task
"""

from __future__ import annotations

from dataclasses import dataclass, replace
from collections import deque
from typing import Callable, Iterable


@dataclass
class Task:
    id: str
    arrival: int
    service: int
    order: int = 0
    remaining: int = 0
    first_run: int | None = None
    completion: int | None = None

    def fresh(self) -> "Task":
        if self.arrival < 0:
            raise ValueError(f"task {self.id}: arrival must be >= 0")
        if self.service < 0:
            raise ValueError(f"task {self.id}: service must be >= 0")
        return replace(
            self,
            remaining=self.service,
            first_run=None,
            completion=None,
        )


@dataclass(frozen=True)
class Slice:
    task: str
    start: int
    end: int


@dataclass
class Result:
    tasks: list[Task]
    timeline: list[Slice]
    cpu_time: int

    def metric(self, task: Task) -> tuple[int, int]:
        if task.service == 0:
            return 0, 0
        assert task.first_run is not None
        assert task.completion is not None
        return task.first_run - task.arrival, task.completion - task.arrival

    def averages(self) -> tuple[float, float]:
        if not self.tasks:
            return 0.0, 0.0
        responses = []
        turnarounds = []
        for task in self.tasks:
            response, turnaround = self.metric(task)
            responses.append(response)
            turnarounds.append(turnaround)
        return sum(responses) / len(responses), sum(turnarounds) / len(turnarounds)

    def validate(self) -> None:
        if self.cpu_time != sum(task.service for task in self.tasks):
            raise AssertionError("CPU conservation failed")
        for task in self.tasks:
            if task.service == 0:
                if task.first_run != task.arrival or task.completion != task.arrival:
                    raise AssertionError(f"zero-service task {task.id} must finish at arrival")
                continue
            if task.first_run is None or task.completion is None:
                raise AssertionError(f"task {task.id} did not complete")
            if not (task.arrival <= task.first_run <= task.completion):
                raise AssertionError(f"invalid timestamps for task {task.id}")


def _prepare(tasks: Iterable[Task]) -> list[Task]:
    prepared: list[Task] = []
    for order, task in enumerate(tasks):
        fresh = task.fresh()
        fresh.order = order
        if fresh.service == 0:
            fresh.first_run = fresh.arrival
            fresh.completion = fresh.arrival
        prepared.append(fresh)
    return prepared


def _pending(tasks: list[Task]) -> list[Task]:
    return sorted(
        [task for task in tasks if task.service > 0],
        key=lambda task: (task.arrival, task.order),
    )


def _admit_until(pending: list[Task], ready, clock: int, add: Callable[[Task], None]) -> None:
    while pending and pending[0].arrival <= clock:
        add(pending.pop(0))


def _next_clock(pending: list[Task], clock: int) -> int:
    if not pending:
        return clock
    return max(clock, pending[0].arrival)


def simulate_fcfs(tasks: Iterable[Task]) -> Result:
    all_tasks = _prepare(tasks)
    pending = _pending(all_tasks)
    ready: deque[Task] = deque()
    timeline: list[Slice] = []
    clock = 0
    cpu_time = 0

    while pending or ready:
        if not ready:
            clock = _next_clock(pending, clock)
            _admit_until(pending, ready, clock, ready.append)
        task = ready.popleft()
        if task.first_run is None:
            task.first_run = clock
        start = clock
        clock += task.remaining
        cpu_time += task.remaining
        task.remaining = 0
        task.completion = clock
        timeline.append(Slice(task.id, start, clock))
        _admit_until(pending, ready, clock, ready.append)

    result = Result(all_tasks, timeline, cpu_time)
    result.validate()
    return result


def simulate_sjf(tasks: Iterable[Task]) -> Result:
    all_tasks = _prepare(tasks)
    pending = _pending(all_tasks)
    ready: list[Task] = []
    timeline: list[Slice] = []
    clock = 0
    cpu_time = 0

    while pending or ready:
        if not ready:
            clock = _next_clock(pending, clock)
        _admit_until(pending, ready, clock, ready.append)
        ready.sort(key=lambda task: (task.service, task.arrival, task.order))
        task = ready.pop(0)
        if task.first_run is None:
            task.first_run = clock
        start = clock
        clock += task.remaining
        cpu_time += task.remaining
        task.remaining = 0
        task.completion = clock
        timeline.append(Slice(task.id, start, clock))

    result = Result(all_tasks, timeline, cpu_time)
    result.validate()
    return result


def simulate_rr(tasks: Iterable[Task], quantum: int) -> Result:
    if quantum <= 0:
        raise ValueError("RR quantum must be > 0")

    all_tasks = _prepare(tasks)
    pending = _pending(all_tasks)
    ready: deque[Task] = deque()
    timeline: list[Slice] = []
    clock = 0
    cpu_time = 0

    while pending or ready:
        if not ready:
            clock = _next_clock(pending, clock)
            _admit_until(pending, ready, clock, ready.append)

        task = ready.popleft()
        if task.first_run is None:
            task.first_run = clock

        run_for = min(quantum, task.remaining)
        start = clock
        clock += run_for
        cpu_time += run_for
        task.remaining -= run_for
        timeline.append(Slice(task.id, start, clock))

        # Required lesson ordering at a time boundary:
        # complete -> admit new arrivals -> requeue expired task -> choose next.
        if task.remaining == 0:
            task.completion = clock
        _admit_until(pending, ready, clock, ready.append)
        if task.remaining > 0:
            ready.append(task)

    result = Result(all_tasks, timeline, cpu_time)
    result.validate()
    return result


def format_result(name: str, result: Result) -> str:
    lines = [name]
    for item in result.timeline:
        lines.append(f"  {item.task} {item.start}-{item.end}")
    for task in sorted(result.tasks, key=lambda task: task.order):
        response, turnaround = result.metric(task)
        lines.append(
            f"  {task.id}: first={task.first_run} completion={task.completion} "
            f"response={response} turnaround={turnaround}"
        )
    response_avg, turnaround_avg = result.averages()
    lines.append(f"  avg_response={response_avg:.6g}")
    lines.append(f"  avg_turnaround={turnaround_avg:.6g}")
    lines.append(f"  cpu_time={result.cpu_time}")
    return "\n".join(lines)


def lesson_example() -> None:
    tasks = [Task("A", 0, 6), Task("B", 0, 2), Task("C", 0, 1)]
    for name, result in [
        ("FCFS", simulate_fcfs(tasks)),
        ("SJF", simulate_sjf(tasks)),
        ("RR(q=2)", simulate_rr(tasks, 2)),
    ]:
        print(format_result(name, result))


if __name__ == "__main__":
    lesson_example()
