"""QEMU integration: independently check switch outcomes and user arithmetic."""
from user_batch import run
from pathlib import Path
import re
import subprocess
import struct


def timebase():
    # Read the generated platform DTB, not a guessed clock frequency.
    out=Path('target/test-output')
    out.mkdir(parents=True,exist_ok=True)
    dtb=out/'virt.dtb'
    subprocess.run(['qemu-system-riscv64','-machine',f'virt,dumpdtb={dtb}',
                    '-smp','1','-nographic'],check=True,capture_output=True,timeout=5)
    data=dtb.read_bytes()
    magic,total,off_struct,off_strings,*_=struct.unpack_from('>10I',data)
    assert magic==0xd00dfeed and total==len(data)
    pos=off_struct
    frequency=None
    while True:
        token=struct.unpack_from('>I',data,pos)[0];pos+=4
        if token==1:
            pos=(data.index(0,pos)+4)&~3
        elif token==3:
            length,nameoff=struct.unpack_from('>II',data,pos);pos+=8
            start=off_strings+nameoff
            name=data[start:data.index(0,start)]
            if name==b'timebase-frequency':
                assert length==4
                frequency=struct.unpack_from('>I',data,pos)[0]
            pos=(pos+length+3)&~3
        elif token in (2,4):pass
        elif token==9:break
        else:raise AssertionError(f'bad FDT token {token}')
    assert frequency and frequency>0
    (out/'timebase.txt').write_text(f'QEMU virt DTB timebase-frequency={frequency} Hz\nQuantum T=10000 platform ticks\n')
    print(f'QEMU DTB timebase-frequency={frequency} Hz')


def main():
    timebase()
    for lesson,feature in [(12,'yield'),(13,'timer'),(14,'preemption'),(15,'mlfq')]:
        text=run(f'lesson{lesson}-{feature}','[schedule] stage-complete',f'schedule{lesson}')
        assert not any(x in text for x in ('[panic]','[trap]','BAD_')),text
        assert text.count('[schedule] stage-complete')==1,text
        assert text.count('begin=')==text.count('stack_stable=true timer_off=true'),text
        if lesson==12:
            assert '\nABABAB\n' in text,text
            assert 'reason=Yield' in text and 'reason=Fault(2)' in text,text
            stress=text.split('[schedule] begin=stress')[1].split('[schedule] begin=fault')[0]
            assert stress.count('yields=500 ')==2,text
        elif lesson==13:
            assert 'id=0 timers=1 ' in text and 'id=0 timers=10 ' in text,text
            assert 'yields=0 ' in text,text
        else:
            compute=text.split('[schedule] begin=compute')[1].split('[schedule] begin=single')[0]
            timers=re.findall(r'\[switch\] task=(\d+) reason=Timer',compute)
            assert '0' in timers and '1' in timers and any(a!=b for a,b in zip(timers,timers[1:])),text
            assert 'reason=Fault(2)' in text,text
            ordinary=text.split('[schedule] begin=syscall')[1]
            row=re.search(r'\[task\] id=0 timers=(\d+) yields=0 syscalls=(\d+) demotions=\d+ preemptions=(\d+)',ordinary)
            assert row and int(row[3])>0 and int(row[2])==5000,text
            if lesson==15:
                comparison=text.split('[schedule] begin=compare-mlfq')[1]
                assert re.search(r'id=0 .*demotions=[1-9]',comparison),text
        print(f'lesson-{lesson} real QEMU checkpoint passed')


if __name__=='__main__': main()
