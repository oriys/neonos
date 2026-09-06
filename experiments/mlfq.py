"""Lesson 15: integer-tick MLFQ, with voluntary yields and simulated I/O.
T=2 model ticks; Q0=(slice 2, allotment 4), Q1=(4,8), Q2=(8,infinite).
Arrival/wakeup ties follow input order. Accounting survives yield and blocking.
"""
from dataclasses import dataclass
import json


@dataclass(frozen=True)
class Job:
    name: str
    arrival: int
    service: int
    burst: int = 0       # zero means no voluntary scheduling point
    wait: int = 0        # zero wait + burst means yield


def simulate(jobs, policy='mlfq', boost_period=40):
    if policy not in ('rr', 'mlfq') or boost_period <= 0:
        raise ValueError('invalid policy/boost period')
    if len({j.name for j in jobs}) != len(jobs) or any(min(j.arrival,j.service,j.burst,j.wait)<0 or (j.wait and not j.burst) for j in jobs):
        raise ValueError('invalid workload')
    states = [dict(left=j.service, level=0, slice=2, allot=4, used=0,
                   wake=j.arrival, state='new', first=None, done=None, ready_at=0, max_wait=0)
              for j in jobs]
    ready, events, trace = [], [], []
    now, next_boost, current, switches = 0, boost_period, None, 0
    while any(s['done'] is None for s in states):
        for i, s in enumerate(states):
            if s['state'] in ('new', 'blocked') and s['wake'] <= now:
                if not s['left']:
                    s.update(first=now, done=now, state='done')
                else:
                    s.update(state='ready', ready_at=now)
                    ready.append(i)
        if policy == 'mlfq' and now >= next_boost:
            for s in states:
                if s['done'] is None:
                    s.update(level=0, slice=2, allot=4)
            events.append((now, 'boost', None))
            next_boost = (now//boost_period+1)*boost_period
        if current is None and ready:
            i = min(ready, key=lambda i: states[i]['level']) if policy == 'mlfq' else ready[0]
            ready.remove(i)
            current = i
            s = states[i]
            s['max_wait'] = max(s['max_wait'], now-s['ready_at'])
            s['state'] = 'running'
            if s['first'] is None:
                s['first'] = now
            switches += 1
        if current is None:
            future = [s['wake'] for s in states if s['state'] in ('new','blocked')]
            if future:
                now = min(future)
            continue
        i, s, j = current, states[current], jobs[current]
        trace.append((now, j.name, s['level']))
        now += 1
        s['left'] -= 1
        s['used'] += 1
        s['slice'] -= 1
        if s['level'] != 2:
            s['allot'] -= 1
        finished = s['left'] == 0
        voluntary = bool(j.burst and s['used'] % j.burst == 0)
        expired = s['slice'] == 0
        if policy == 'mlfq' and s['level'] < 2 and s['allot'] == 0:
            s['level'] += 1
            s['slice'] = (2,4,8)[s['level']]
            s['allot'] = (4,8,0)[s['level']]
            events.append((now,'demote',j.name))
            expired = True
        elif expired:
            s['slice'] = (2,4,8)[s['level']] if policy == 'mlfq' else 2
        # Completion/block before arrivals; current is appended after arrivals.
        if finished:
            s.update(done=now,state='done')
        elif voluntary and j.wait:
            s.update(state='blocked',wake=now+j.wait)
        # Queue new arrivals first on this boundary.
        for k, other in enumerate(states):
            if other['state'] in ('new','blocked') and other['wake'] <= now:
                if other['left']:
                    other.update(state='ready',ready_at=now)
                    ready.append(k)
                else:
                    other.update(first=now,done=now,state='done')
        higher = policy == 'mlfq' and any(states[k]['level'] < s['level'] for k in ready)
        boosted = policy == 'mlfq' and now >= next_boost
        if finished or voluntary or expired or higher or boosted:
            if s['state'] == 'running':
                s.update(state='ready',ready_at=now)
                ready.append(i)
            current = None
        assert len(ready) == len(set(ready))
        assert all(states[k]['state']=='ready' for k in ready)
    assert len(trace) == sum(j.service for j in jobs)
    metrics = {j.name: dict(response=s['first']-j.arrival, turnaround=s['done']-j.arrival,
                           max_ready_wait=s['max_wait']) for j,s in zip(jobs,states)}
    return dict(jobs=metrics, switches=switches, events=events, trace=trace)


if __name__ == '__main__':
    jobs = [Job('compute',0,60), Job('short',5,3), Job('io',0,12,2,6), Job('yield',0,30,1)]
    for policy in ('rr','mlfq'):
        result = simulate(jobs,policy)
        result.pop('trace')
        print(json.dumps(dict(policy=policy, **result)))
