"""Lesson 11: deterministic FCFS, non-preemptive SJF, and RR.

Boundary order: completion, arrivals in input order, old RR task, selection.
No context-switch overhead; all times are integer model ticks.
"""
from dataclasses import dataclass
from collections import deque
import json


@dataclass(frozen=True)
class Job:
    name: str
    arrival: int
    service: int


def simulate(jobs, policy='rr', quantum=2):
    if policy not in ('fcfs', 'sjf', 'rr') or quantum <= 0:
        raise ValueError('invalid policy/quantum')
    if len({j.name for j in jobs}) != len(jobs):
        raise ValueError('duplicate job id')
    if any(j.arrival < 0 or j.service < 0 for j in jobs):
        raise ValueError('negative arrival/service')
    pending = deque(sorted(range(len(jobs)), key=lambda i: (jobs[i].arrival, i)))
    ready, remaining, first, done, trace = [], [j.service for j in jobs], {}, {}, []
    now = 0

    def arrive():
        while pending and jobs[pending[0]].arrival <= now:
            i = pending.popleft()
            if jobs[i].service == 0:
                first[i] = done[i] = jobs[i].arrival
            else:
                ready.append(i)

    while pending or ready:
        if not ready:
            now = max(now, jobs[pending[0]].arrival)
        arrive()
        if not ready:
            continue
        if policy == 'sjf':
            i = min(ready, key=lambda i: (remaining[i], jobs[i].arrival, i))
            ready.remove(i)
        else:
            i = ready.pop(0)
        first.setdefault(i, now)
        duration = min(remaining[i], quantum) if policy == 'rr' else remaining[i]
        trace.append((jobs[i].name, now, now + duration))
        now += duration
        remaining[i] -= duration
        if remaining[i] == 0:
            done[i] = now
        arrive()
        if remaining[i]:
            ready.append(i)
    assert sum(b-a for _, a, b in trace) == sum(j.service for j in jobs)
    result = {j.name: {'response': first[i]-j.arrival, 'turnaround': done[i]-j.arrival}
              for i, j in enumerate(jobs)}
    assert all(v['response'] >= 0 and v['turnaround'] >= v['response'] for v in result.values())
    return result, trace


if __name__ == '__main__':
    jobs = [Job('A', 0, 6), Job('B', 0, 2), Job('C', 0, 1)]
    for policy in ('fcfs', 'sjf', 'rr'):
        result, trace = simulate(jobs, policy)
        print(json.dumps({'policy': policy, 'jobs': result, 'trace': trace}))
