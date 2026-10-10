import concurrent.futures
import datetime
import json
import os
import pathlib
import subprocess
import urllib.request

OUT = pathlib.Path('D:/training-speedups-20261009')
OUT.mkdir(exist_ok=True)
ROOT = pathlib.Path('C:/Users/Jack/.codex/worktrees/training-throughput-audit-20261009/mtg-kernel')
PS = r'''
$cpu = Get-CimInstance Win32_Processor | Select-Object Name,NumberOfCores,NumberOfLogicalProcessors,LoadPercentage
$mem = Get-CimInstance Win32_OperatingSystem | Select-Object TotalVisibleMemorySize,FreePhysicalMemory
$disk = Get-CimInstance Win32_LogicalDisk -Filter 'DriveType=3' | Select-Object DeviceID,Size,FreeSpace
$locks = @(Get-ChildItem -LiteralPath C:/mtg-node/host-lock -Filter '*.lock' -File | ForEach-Object { $r=Get-Content -LiteralPath $_.FullName -Raw | ConvertFrom-Json; $r | Select-Object schema,host,generation,lane,work_id,acquired_at,release_condition })
$gpu = & nvidia-smi --query-gpu=index,name,driver_version,memory.total,memory.used,utilization.gpu --format=csv
@{observed_utc=[DateTime]::UtcNow.ToString('o'); host=$env:COMPUTERNAME; cpu=@($cpu);memory=$mem;volumes=@($disk);reservations=$locks;gpu=@($gpu)} | ConvertTo-Json -Depth 6 -Compress
'''

def local():
    p = subprocess.run(['powershell','-NoProfile','-Command',PS],text=True,capture_output=True,timeout=40,check=True)
    data = json.loads(p.stdout)
    data['slots'] = json.loads(subprocess.check_output(['python','-B',str(ROOT/'python/tools/host_slots_v1.py'),'status'],text=True))
    return data

def remote():
    code = 'import subprocess\np=subprocess.run('+repr(['powershell','-NoProfile','-Command',PS])+',capture_output=True,text=True,timeout=40)\nprint(p.stdout)\nraise SystemExit(p.returncode)\n'
    p = subprocess.run(['ssh','-o','BatchMode=yes','-o','ConnectTimeout=10','haley@100.71.75.65','C:/Users/haley/AppData/Local/Programs/Python/Python312/python.exe','-B','-'],input=code,text=True,capture_output=True,timeout=55,check=True)
    return json.loads(p.stdout)

def cloud():
    key = os.environ.get('RUNPOD_API_KEY')
    if not key:
        import winreg
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER,'Environment') as h:
            key=winreg.QueryValueEx(h,'RUNPOD_API_KEY')[0]
    req=urllib.request.Request('https://rest.runpod.io/v1/pods',headers={'Authorization':'Bearer '+key})
    with urllib.request.urlopen(req,timeout=30) as response:
        rows=json.load(response)
    return {'observed_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'paid_authority':False,'pods':[{k:r.get(k) for k in ('id','name','desiredStatus','machineId','costPerHr')} for r in rows]}

with concurrent.futures.ThreadPoolExecutor(max_workers=3) as pool:
    jobs={name:pool.submit(fn) for name,fn in [('desktop',local),('computehost',remote),('runpod',cloud)]}
    data={}
    for name,job in jobs.items():
        try: data[name]=job.result()
        except Exception as exc: data[name]={'error':type(exc).__name__,'message':str(exc)[:300]}
path=OUT/'inventory-20261009.json'
path.write_text(json.dumps(data,indent=2)+'\n')
print(json.dumps({'path':str(path),'inventory':data}))
