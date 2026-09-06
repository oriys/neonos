"""Run complete QEMU sessions; retain logs, reject early/duplicate success."""
from pathlib import Path
import subprocess
import time
import os


def run(features, final, name):
    out = Path('target/test-output')
    out.mkdir(parents=True, exist_ok=True)
    profile = os.environ.get('NEONOS_PROFILE', 'debug')
    assert profile in ('debug', 'release')
    name = f'{name}-{profile}'
    command = ['cargo', 'build', '--features', features]
    if profile == 'release': command.append('--release')
    build = subprocess.run(command, capture_output=True, text=True)
    (out / f'{name}-build.log').write_text(build.stdout + build.stderr)
    assert build.returncode == 0, build.stderr
    path = out / f'{name}.log'
    # Tie scheduling fixture time to executed guest instructions, not host speed.
    clock_args = ['-icount', 'shift=0,align=off,sleep=off'] if name.startswith('schedule') else []
    with path.open('w') as log:
        p = subprocess.Popen(['qemu-system-riscv64', '-machine', 'virt', '-bios', 'default',
                              '-kernel', f'target/riscv64gc-unknown-none-elf/{profile}/neonos',
                              '-nographic', '-smp', '1'] + clock_args, stdout=log, stderr=subprocess.STDOUT)
        try:
            deadline = time.monotonic() + 15
            while final not in path.read_text() and time.monotonic() < deadline:
                assert p.poll() is None, path.read_text()
                time.sleep(.05)
            time.sleep(.3)  # Capture failures after the final/diagnostic marker too.
            text = path.read_text()
            assert final in text and p.poll() is None, text
        finally:
            p.terminate()
            try:
                p.wait(timeout=3)
            except subprocess.TimeoutExpired:
                p.kill()
                p.wait()
    return text


def main():
    text = run('lesson10-user-errors', '[user batch] stage-complete', 'user10')
    try:
        markers = ['[user error] syscall=2 arg=256 result=-2', '[user batch] Exited(7)',
                   '[user fault] pid=4 program=illegal cause=2',
                   '[user error] syscall=999 arg=0 result=-1',
                   '[user error] syscall=1 arg=256 result=-2', '\nOK\n', '\nAFTER_FAULT_OK\n',
                   '[user batch] completed=100 stack_stable=true', '[user batch] stage-complete']
        positions = []
        for marker in markers:
            assert text.count(marker) == 1, marker
            positions.append(text.index(marker))
        assert positions == sorted(positions)
        assert not any(x in text for x in ['[panic]', '[trap]', 'BAD_', 'failure'])
    except AssertionError:
        print(text)
        raise
    text = run('lesson10-kernel-fault', 'trigger_match=true', 'user10-kernel-fault')
    assert 'scause=0x2' in text and 'cause_code=2' in text
    assert text.count('[trap]') == 1
    assert '[user batch]' not in text and '[user fault]' not in text
    # SPP must prove that the nested fault happened in S-mode with a process active.
    status = next(line for line in text.splitlines() if line.startswith('sstatus='))
    assert int(status.split('=')[1], 16) & 0x100
    text = run('lesson03-panic', '[panic] lesson 03 deliberate failure', 'user10-panic')
    assert 'at src/main.rs:' in text and 'SHOULD_NOT_REACH' not in text
    print('lesson-10 user batch and kernel fault/panic boundaries passed')


if __name__ == '__main__':
    main()
