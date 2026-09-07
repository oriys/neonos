#!/usr/bin/env python3
"""Create isolated student labs and grade against frozen instructor fixtures.

Requires a full clone of the course repository and Python 3.9+, Rust and QEMU.
No branch switches or writes to the instructor's kernel sources are performed.
"""
import argparse
import ast
import hashlib
import fcntl
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import zipfile
from lab_catalog import BASE, LABS

ROOT = Path(__file__).resolve().parents[1]


def git(*args):
    return subprocess.check_output(['git', '-C', str(ROOT), *args])


def snapshot(dest):
    """Only copy regular tracked project files from the immutable base commit."""
    for entry in git('ls-tree', '-rz', BASE).split(b'\0'):
        if not entry:
            continue
        info, raw_name = entry.split(b'\t', 1)
        mode, kind, oid = info.split()
        name = raw_name.decode()
        if name.startswith(('.github/', 'docs/superpowers/')):
            continue
        if kind != b'blob' or mode not in (b'100644', b'100755'):
            raise ValueError(f'unsupported snapshot entry: {name}')
        path = dest / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(git('cat-file', 'blob', oid.decode()))
        path.chmod(0o755 if mode == b'100755' else 0o644)


def remove_body(source, name, replacement, language):
    if language == 'python':
        nodes = [n for n in ast.parse(source).body if isinstance(n, ast.FunctionDef) and n.name == name]
        if len(nodes) != 1:
            raise ValueError(f'expected one Python function {name}')
        node = nodes[0]
        lines = source.splitlines(keepends=True)
        # The frozen lab functions have single-line signatures.
        assert lines[node.lineno - 1].rstrip().endswith(':')
        return ''.join(lines[:node.lineno]) + '    ' + replacement + '\n' + ''.join(lines[node.end_lineno:])
    # Frozen Rust functions are formatted with their closing brace at signature
    # indentation. This is deliberately a checked template edit, not a Rust parser.
    pattern = rf'^(?P<indent> *)(?:pub(?:\([^\n]*?\))? )?(?:extern "C" )?fn {re.escape(name)}\b[^\n]*'
    matches = list(re.finditer(pattern, source, re.M))
    if len(matches) != 1:
        raise ValueError(f'expected one Rust function {name}')
    match = matches[0]
    begin = source.index('{', match.start())
    end = re.search(r'^' + match['indent'] + r'}\s*$', source[begin:], re.M)
    if not end:
        raise ValueError(f'missing closing brace for {name}')
    close = begin + end.start()
    indent = match['indent'] + '    '
    body = '\n'.join(indent + line.lstrip() for line in replacement.splitlines())
    return source[:begin+1] + '\n' + body + '\n' + source[close:]


def skeleton(dest, spec):
    for language, name, symbol, replacement in spec['holes']:
        path = dest / name
        text = path.read_text()
        if language == 'line':
            if text.count(symbol) != 1:
                raise ValueError(f'non-unique template marker: {symbol}')
            text = text.replace(symbol, replacement)
        else:
            text = remove_body(text, symbol, replacement, language)
        path.write_text(text)


def start(lab, dest, reference=False):
    dest = dest.expanduser().resolve()
    if dest.exists():
        raise ValueError(f'目录已存在，拒绝覆盖作业：{dest}')
    for parent in dest.parents:
        if (parent / "Cargo.toml").exists() and any((parent / ".cargo" / name).exists() for name in ("config", "config.toml")):
            raise ValueError("请选择 Cargo 项目以外的独立目录，避免父级编译配置叠加")
    dest.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='.neonos-new-', dir=dest.parent) as temp:
        work = Path(temp) / 'work'
        work.mkdir()
        snapshot(work)
        if not reference:
            skeleton(work, LABS[lab])
        for name in ('LAB.md', 'HINTS.md', 'answers.md'):
            shutil.copyfile(ROOT / 'labs' / lab / name, work / name)
        (work / '.neonos-lab.json').write_text(json.dumps({'version': 1, 'lab': lab, 'base': BASE, 'reference': reference}, indent=2)+'\n')
        tool = shlex.quote(str(ROOT / 'tools/lab.py')).replace('$', '$$')
        (work / 'Makefile').write_text(
            '.PHONY: help grade submit\n'
            'help:\n\t@echo "阅读 LAB.md；make grade；make grade PART=console；make submit"\n'
            f'grade:\n\tpython3 {tool} grade --workspace . $(if $(PART),--part "$(PART)",)\n'
            f'submit:\n\tpython3 {tool} submit --workspace .\n')
        subprocess.run(['git', 'init', '-q', '-b', 'work'], cwd=work, check=True)
        subprocess.run(['git', 'add', '.'], cwd=work, check=True)
        subprocess.run(['git', '-c', 'user.name=neonos lab starter', '-c', 'user.email=lab@localhost',
                        '-c', 'commit.gpgsign=false', 'commit', '-qm', f'{lab}: immutable starter'], cwd=work, check=True)
        work.rename(dest)
    print(f'{lab} {"参考实现" if reference else "学生作业"}已创建：{dest}\n阅读：{dest / "LAB.md"}\n开始：cd {shlex.quote(str(dest))} && make grade')
    return dest


def workspace(path):
    path = path.expanduser().resolve()
    meta = json.loads((path / '.neonos-lab.json').read_text())
    if meta.get('version') != 1 or meta.get('base') != BASE or meta.get('lab') not in LABS:
        raise ValueError('作业元数据不匹配本版课程')
    return path, meta, LABS[meta['lab']]


def read_student(path, name):
    file = path / name
    if file.is_symlink() or not file.resolve().is_relative_to(path) or not file.is_file():
        raise ValueError(f'作业文件必须是目录内的普通文件：{name}')
    return file.read_bytes()


def payload_fingerprint(spec, payload):
    h = hashlib.sha256()
    for name in spec['editable']:
        h.update(name.encode()+b'\0'+payload[name]+b'\0')
    return h.hexdigest()


def fingerprint(path, spec):
    return payload_fingerprint(spec, {name: read_student(path, name) for name in spec['editable']})


def output_dir(path):
    path = path.resolve()
    target = path / 'target' / 'lab-grades'
    if not target.resolve().is_relative_to(path):
        raise ValueError('拒绝向作业目录外的 target 链接写入')
    target.mkdir(parents=True, exist_ok=True)
    return target


def execute(command, cwd, log, timeout=120):
    env = os.environ.copy()
    # Never recursively enter the repository-wide CI suite or reuse caller flags.
    for key in ('GITHUB_ACTIONS', 'CI', 'CARGO_TARGET_DIR', 'NEONOS_PROFILE', 'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS'):
        env.pop(key, None)
    env['CARGO_ENCODED_RUSTFLAGS'] = '-C\x1flink-arg=-Tlinker.ld'
    with log.open('w') as out:
        process = subprocess.Popen(command, cwd=cwd, env=env, stdout=out, stderr=subprocess.STDOUT, start_new_session=True)
        try:
            code = process.wait(timeout=timeout)
            return code == 0, 'PASS' if code == 0 else f'FAIL (exit {code})'
        except subprocess.TimeoutExpired:
            return False, f'TIMEOUT ({timeout}s)'
        finally:
            # Every grader owns one process group, including any spawned QEMU.
            try:
                os.killpg(process.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            try:
                process.wait(timeout=2)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()


def grade(path, part=None):
    path, _, _ = workspace(path)
    with (output_dir(path) / '.grade.lock').open('w') as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise ValueError('这个作业正在评分，请等待完成后再试')
        return grade_locked(path, part)


def grade_locked(path, part=None):
    path, meta, spec = workspace(path)
    parts = [p for p in spec['parts'] if part is None or p[0] == part]
    if not parts:
        raise ValueError('未知分项；可选：' + ', '.join(p[0] for p in spec['parts']))
    outputs = output_dir(path)
    digest = fingerprint(path, spec)
    result = {'lab': meta['lab'], 'base': BASE, 'source_sha256': digest,
              'reference': meta['reference'], 'time': time.time(), 'parts': [], 'score': 0,
              'total': sum(p[1] for p in parts), 'complete': False}
    with tempfile.TemporaryDirectory(prefix='neonos-grade-') as temp:
        work = Path(temp)
        snapshot(work)
        for name in spec['editable']:
            (work / name).write_bytes(read_student(path, name))
        # Keep generated binary/log evidence between grades without trusting the
        # learner's tests. The scripts and fixtures above came from BASE.
        cache = outputs / 'build'
        if not cache.resolve().is_relative_to(path):
            raise ValueError("评分缓存不能指向作业目录外")
        cache.mkdir(exist_ok=True)
        (work / 'target').symlink_to(cache, target_is_directory=True)
        # Instructor adapter adds only selection; behavior assertions are unchanged.
        adapter = ROOT / 'labs' / 'scheduling_batch.py'
        shutil.copyfile(adapter, work / 'tests/scheduling_batch.py')
        for label, points, command in parts:
            log = outputs / f'{label}.log'
            passed, status = execute(command, work, log)
            earned = points if passed else 0
            result['score'] += earned
            result['parts'].append(dict(part=label, passed=passed, score=earned, total=points, status=status))
            print(f'{label:12} {status:20} {earned}/{points}  日志：{log}', flush=True)
    if fingerprint(path, spec) != digest:
        raise ValueError('评分期间作业被修改，请重新评分')
    result['complete'] = part is None and result['score'] == result['total']
    report = outputs / ('grade.json' if part is None else f'grade-{part}.json')
    report.write_text(json.dumps(result, ensure_ascii=False, indent=2)+'\n')
    print(f'自动测试：{result["score"]}/{result["total"]}。answers.md 的解释题需自行复盘/人工审阅。')
    return result


def submit(path):
    path, meta, spec = workspace(path)
    outputs = output_dir(path)
    payload = {name: read_student(path, name) for name in [*spec['editable'], 'answers.md']}
    digest = payload_fingerprint(spec, payload)
    report_path = outputs / 'grade.json'
    report = json.loads(report_path.read_text()) if report_path.exists() else None
    current = bool(report and report.get('source_sha256') == digest)
    package = outputs / f'{meta["lab"]}-{time.time_ns()}.zip'
    with zipfile.ZipFile(package, 'w', zipfile.ZIP_DEFLATED) as archive:
        for name, contents in payload.items():
            archive.writestr(name, contents)
        archive.writestr('submission.json', json.dumps(dict(**meta, source_sha256=digest,
                            grade_current=current, grade=report if current else None), ensure_ascii=False, indent=2))
    print(f'已生成本地作业包：{package}\n评分记录：{"与代码一致" if current else "缺失或已过期，请重新 make grade"}。未上传到任何服务。')
    return package


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    commands.add_parser('list')
    commands.add_parser('doctor')
    new = commands.add_parser('start')
    new.add_argument('lab', choices=LABS)
    new.add_argument('--dest', type=Path)
    new.add_argument('--reference', action='store_true', help='显式创建含完整解答的教师核验目录')
    for command in ('grade', 'submit'):
        sub = commands.add_parser(command)
        sub.add_argument('--workspace', type=Path, default=Path.cwd())
        if command == 'grade': sub.add_argument('--part')
    args = parser.parse_args()
    try:
        if args.command == 'doctor':
            missing = [name for name in ('git', 'cargo', 'rustup', 'qemu-system-riscv64', 'make') if shutil.which(name) is None]
            for name in ('git', 'cargo', 'rustup', 'qemu-system-riscv64', 'make'):
                print(f'{name}: {shutil.which(name) or "未安装"}')
            if missing: raise ValueError('请先完成第 00 课的环境安装')
            targets = subprocess.check_output(['rustup','target','list','--installed'], text=True)
            if 'riscv64gc-unknown-none-elf' not in targets:
                raise ValueError('请执行 rustup target add riscv64gc-unknown-none-elf')
            git('cat-file', '-e', BASE+'^{commit}')
            print('环境及参考快照可用。')
        elif args.command == 'list':
            for key, spec in LABS.items(): print(f'{key}: 第 {spec["lessons"]} 课 · {spec["title"]} · 可开始')
            print('lab4–lab8: 内存、进程接口、并发、存储、恢复；规划中，尚无学生起始代码。')
        elif args.command == 'start': start(args.lab, args.dest or ROOT.parent / f'{ROOT.name}-labs' / args.lab, args.reference)
        elif args.command == 'grade':
            result = grade(args.workspace, args.part)
            return 0 if result['score'] == result['total'] else 1
        else: submit(args.workspace)
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        print(f'lab: {error}', file=sys.stderr)
        return 2
    return 0


if __name__ == '__main__':
    sys.exit(main())
