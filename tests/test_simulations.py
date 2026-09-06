import sys
from pathlib import Path
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'experiments'))
import scheduling as sched
import mlfq


class SchedulingTests(unittest.TestCase):
    def test_hand_calculated_course_example(self):
        jobs = [sched.Job('A',0,6), sched.Job('B',0,2), sched.Job('C',0,1)]
        for policy, response, turnaround in [('fcfs',14,23),('sjf',4,13),('rr',6,18)]:
            data, _ = sched.simulate(jobs,policy)
            self.assertEqual(sum(v['response'] for v in data.values()),response)
            self.assertEqual(sum(v['turnaround'] for v in data.values()),turnaround)
    def test_arrival_before_rr_requeue(self):
        _, trace = sched.simulate([sched.Job('A',0,4),sched.Job('B',2,1)])
        self.assertEqual(trace,[('A',0,2),('B',2,3),('A',3,5)])
    def test_empty_idle_zero_and_invalid(self):
        self.assertEqual(sched.simulate([]),({},[]))
        data, trace = sched.simulate([sched.Job('zero',2,0),sched.Job('late',10,1)])
        self.assertEqual(data['zero'],dict(response=0,turnaround=0))
        self.assertEqual(trace,[('late',10,11)])
        for jobs, q in [([sched.Job('bad',-1,2)],2),([],0),([sched.Job('bad',0,-1)],2)]:
            with self.assertRaises(ValueError): sched.simulate(jobs,quantum=q)
    def test_yield_does_not_reset_allotment(self):
        result = mlfq.simulate([mlfq.Job('gamer',0,20,1)],boost_period=1000)
        self.assertEqual(result['events'],[(4,'demote','gamer'),(12,'demote','gamer')])
        self.assertEqual(result['jobs']['gamer']['turnaround'],20)
    def test_blocked_boost_does_not_wake(self):
        result = mlfq.simulate([mlfq.Job('io',0,2,1,50)],boost_period=10)
        self.assertEqual([tick for tick,_,_ in result['trace']],[0,51])
    def test_deterministic_comparison_and_conservation(self):
        jobs=[mlfq.Job('cpu',0,80),mlfq.Job('short',5,3),mlfq.Job('io',0,8,2,6),mlfq.Job('yield',0,30,1)]
        for policy in ('rr','mlfq'):
            result=mlfq.simulate(jobs,policy)
            self.assertEqual(result,mlfq.simulate(jobs,policy))
            self.assertEqual(len(result['trace']),121)
            self.assertEqual(set(result['jobs']),{j.name for j in jobs})
    def test_boost_after_allotment_expiry(self):
        result=mlfq.simulate([mlfq.Job('A',0,8)],boost_period=4)
        self.assertEqual(result['events'][:2],[(4,'demote','A'),(4,'boost',None)])


if __name__ == '__main__': unittest.main()
