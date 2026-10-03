"""Read live multirun coordinator placements; no locks or process mutation."""
import hashlib,json,pathlib,psutil
P=pathlib.Path
def controllers():
    result=[]
    for proc in psutil.process_iter(['name']):
        if (proc.info['name'] or '').lower() not in ('python.exe','python','python3'):continue
        try:
            argv=proc.cmdline()
            matches=[i for i,a in enumerate(argv) if P(a).name=='multirun_launcher_v1.py']
            if not matches:continue
            index=matches[0]+1
            if len(argv)<=index or argv[index] not in ('qualify','launch'):continue
            mode=argv[index];hosts=set();row=dict(pid=proc.pid,created_unix=proc.create_time(),mode=mode)
            if mode=='qualify':
                allocations=[argv[i+1] for i,a in enumerate(argv[:-1]) if a=='--allocation']
                for allocation in allocations:
                    for item in allocation.split('+'):
                        target=item.split('@',1)[1];hosts.add(target.split(':',1)[0] if ':' in target else 'jack')
            else:
                choice=P(argv[argv.index('--choice')+1])
                if not choice.is_absolute():choice=P(proc.cwd())/choice
                raw=choice.read_bytes();data=json.loads(raw)
                selected=next(c for c in data['candidates'] if c['id']==data['selected'])
                hosts.update(selected['hosts']);row.update(choice_path=str(choice),choice_sha256=hashlib.sha256(raw).hexdigest())
            row['hosts']=sorted(hosts or {'jack','haleyspc'});result.append(row)
        except (psutil.NoSuchProcess,psutil.AccessDenied):continue
        except (OSError,ValueError,KeyError,IndexError,StopIteration) as error:
            result.append(dict(pid=proc.pid,hosts=['jack','haleyspc'],placement_error=str(error)))
    return result
