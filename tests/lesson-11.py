#!/usr/bin/env python3
import sys
from importlib.util import module_from_spec, spec_from_file_location
from pathlib import Path

root = Path(__file__).resolve().parents[1]
spec = spec_from_file_location("scheduling", root / "experiments" / "scheduling.py")
assert spec and spec.loader
scheduling = module_from_spec(spec)
sys.modules[spec.name] = scheduling
spec.loader.exec_module(scheduling)

Task = scheduling.Task


def metric_map(result):
    return {task.id: result.metric(task) for task in result.tasks}


# Textbook example from lesson 11.
tasks = [Task("A", 0, 6), Task("B", 0, 2), Task("C", 0, 1)]
fcfs = scheduling.simulate_fcfs(tasks)
sjf = scheduling.simulate_sjf(tasks)
rr = scheduling.simulate_rr(tasks, 2)

assert [(s.task, s.start, s.end) for s in fcfs.timeline] == [
    ("A", 0, 6), ("B", 6, 8), ("C", 8, 9)
]
assert metric_map(fcfs) == {"A": (0, 6), "B": (6, 8), "C": (8, 9)}
assert abs(fcfs.averages()[0] - 14 / 3) < 1e-12
assert abs(fcfs.averages()[1] - 23 / 3) < 1e-12

assert [(s.task, s.start, s.end) for s in sjf.timeline] == [
    ("C", 0, 1), ("B", 1, 3), ("A", 3, 9)
]
assert metric_map(sjf) == {"A": (3, 9), "B": (1, 3), "C": (0, 1)}
assert abs(sjf.averages()[0] - 4 / 3) < 1e-12
assert abs(sjf.averages()[1] - 13 / 3) < 1e-12

assert [(s.task, s.start, s.end) for s in rr.timeline] == [
    ("A", 0, 2), ("B", 2, 4), ("C", 4, 5), ("A", 5, 7), ("A", 7, 9)
]
assert metric_map(rr) == {"A": (0, 9), "B": (2, 4), "C": (4, 5)}
assert rr.averages() == (2.0, 6.0)

# Arrival prediction exercise and deterministic boundary ordering.
shifted = [Task("A", 0, 6), Task("B", 1, 2), Task("C", 3, 1)]
shifted_fcfs = scheduling.simulate_fcfs(shifted)
assert [(s.task, s.start, s.end) for s in shifted_fcfs.timeline] == [
    ("A", 0, 6), ("B", 6, 8), ("C", 8, 9)
]
shifted_rr = scheduling.simulate_rr(shifted, 2)
# At t=2 B has arrived before expired A is requeued, so B runs next.
assert [(s.task, s.start, s.end) for s in shifted_rr.timeline[:4]] == [
    ("A", 0, 2), ("B", 2, 4), ("A", 4, 6), ("C", 6, 7)
]

# Idle CPU jumps directly to the next arrival.
idle = scheduling.simulate_fcfs([Task("A", 5, 2)])
assert [(s.task, s.start, s.end) for s in idle.timeline] == [("A", 5, 7)]
assert metric_map(idle) == {"A": (0, 2)}

# Zero-service tasks complete at arrival without consuming CPU.
zero = scheduling.simulate_rr([Task("Z", 7, 0), Task("A", 10, 1)], 2)
assert metric_map(zero)["Z"] == (0, 0)
assert zero.cpu_time == 1

# Stable input-order tie breaking.
tie = scheduling.simulate_sjf([Task("A", 0, 2), Task("B", 0, 2)])
assert [s.task for s in tie.timeline] == ["A", "B"]

# Required invalid-input behavior.
for bad in ([Task("A", -1, 1)], [Task("A", 0, -1)]):
    try:
        scheduling.simulate_fcfs(bad)
    except ValueError:
        pass
    else:
        raise AssertionError("negative arrival/service must be rejected")

for quantum in (0, -1):
    try:
        scheduling.simulate_rr([Task("A", 0, 1)], quantum)
    except ValueError:
        pass
    else:
        raise AssertionError("non-positive RR quantum must be rejected")

# Empty inputs are valid and conserve zero CPU time.
for simulator in (scheduling.simulate_fcfs, scheduling.simulate_sjf):
    result = simulator([])
    assert result.timeline == [] and result.averages() == (0.0, 0.0)
empty_rr = scheduling.simulate_rr([], 2)
assert empty_rr.timeline == [] and empty_rr.cpu_time == 0

print("lesson-11 scheduling tests passed")
