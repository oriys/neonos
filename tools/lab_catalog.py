"""Versioned lab specifications. New reference commits require instructor verification."""
BASE = '4085c7d6cdc4a1e3289d70a9db7d9bbc04b5bf96'
LABS = {
    'lab1': {
        'title': '内核基础：输出、panic、BSS 与 trap', 'lessons': '01–05',
        'editable': ['src/console.rs', 'src/main.rs', 'src/trap.rs'],
        'holes': [
            ('rust', 'src/console.rs', '_print', 'let _ = args; // TODO(lab1.console): connect fmt::Arguments to Console'),
            ('rust', 'src/main.rs', 'panic', 'let _ = info; // TODO(lab1.panic): print message and location\n    halt()'),
            ('line', 'src/main.rs', '    sb zero, 0(t0)', '    nop # TODO(lab1.bss): clear the byte at t0'),
            ('rust', 'src/trap.rs', 'init', '// TODO(lab1.trap): disable supervisor interrupts and install/read back stvec'),
        ],
        'parts': [
            ('console', 25, ['sh', 'tests/lesson-02.sh']),
            ('panic', 25, ['sh', 'tests/lesson-03.sh']),
            ('bss', 25, ['sh', 'tests/lesson-04.sh']),
            ('trap', 25, ['sh', 'tests/lesson-05.sh']),
        ],
    },
    'lab2': {
        'title': '用户程序：进程状态、syscall、返回与故障', 'lessons': '06–10',
        'editable': ['src/process.rs', 'src/syscall.rs', 'src/user.rs', 'src/user10.rs'],
        'holes': [
            ('rust', 'src/process.rs', 'transition', 'Err(InvalidTransition { from: self.state, to: next }) // TODO(lab2.process)'),
            ('rust', 'src/syscall.rs', 'dispatch', 'let _ = (number, arg0);\n    SyscallOutcome::Return(ERR_UNKNOWN_SYSCALL) // TODO(lab2.syscall)'),
            ('rust', 'src/user.rs', 'apply_return', 'let _ = (frame, result, code_end); // TODO(lab2.return): modify the saved user context'),
            ('rust', 'src/user10.rs', 'handle', 'let _ = frame;\n    panic!("TODO(lab2.fault): classify origin/cause and return a run outcome")'),
        ],
        'parts': [
            ('process', 20, ['sh', 'tests/lesson-06.sh']),
            ('syscall', 25, ['sh', 'tests/lesson-08.sh']),
            ('exit', 25, ['sh', 'tests/lesson-09.sh']),
            ('fault', 30, ['python3', 'tests/user_batch.py']),
        ],
    },
    'lab3': {
        'title': 'CPU 调度：模拟、Ready 队列、timer 与 MLFQ', 'lessons': '11–15',
        'editable': ['experiments/scheduling.py', 'experiments/mlfq.py', 'src/scheduler.rs', 'src/timer.rs'],
        'holes': [
            ('python', 'experiments/scheduling.py', 'simulate', 'raise NotImplementedError("TODO(lab3.model): FCFS/SJF/RR")'),
            ('python', 'experiments/mlfq.py', 'simulate', 'raise NotImplementedError("TODO(lab3.mlfq): budgets, blocking and boost")'),
            ('rust', 'src/scheduler.rs', 'push', 'let _ = id;\n        Err("TODO(lab3.queue): enqueue a task without duplicates")'),
            ('rust', 'src/timer.rs', 'arm', 'let _ = deadline; // TODO(lab3.timer): program SBI timer before enabling STIE'),
        ],
        'parts': [
            ('model', 20, ['python3', 'tests/test_simulations.py',
                'SchedulingTests.test_hand_calculated_course_example',
                'SchedulingTests.test_arrival_before_rr_requeue',
                'SchedulingTests.test_empty_idle_zero_and_invalid']),
            ('mlfq', 20, ['python3', 'tests/test_simulations.py',
                'SchedulingTests.test_yield_does_not_reset_allotment',
                'SchedulingTests.test_blocked_boost_does_not_wake',
                'SchedulingTests.test_deterministic_comparison_and_conservation',
                'SchedulingTests.test_completion_precedes_demotion',
                'SchedulingTests.test_boost_after_allotment_expiry']),
            ('yield', 20, ['python3', 'tests/scheduling_batch.py', '--lesson', '12']),
            ('timer', 20, ['python3', 'tests/scheduling_batch.py', '--lesson', '13']),
            ('preemption', 20, ['python3', 'tests/scheduling_batch.py', '--lesson', '14', '--lesson', '15']),
        ],
    },
}
