"""WMI-owned, single-shot transfer of the declared D4 archive packet to the compute host."""
import argparse
import base64
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import subprocess
import time

REMOTE = os.environ.get('COMPUTE_HOST_SSH', 'compute-host')
ROOT = 'C:/mtg-node/g115-d4-audit-production-20260925-001'
RESERVE = 60*1024**3


def require(ok, message):
    if not ok:
        raise ValueError(message)


def pin(path):
    with Path(path).open('rb') as f:
        return {'path':str(path),'sha256':hashlib.file_digest(f,'sha256').hexdigest()}


def checked(ref):
    require(pin(ref['path'])['sha256'] == ref['sha256'], 'Pinned transfer control changed')
    return Path(ref['path'])


def save(path, value):
    with path.open('x', encoding='utf-8') as f:
        json.dump(value,f,indent=2)
        f.flush();os.fsync(f.fileno())


def ssh(script, timeout=120):
    encoded=base64.b64encode(("$ErrorActionPreference='Stop'\n$ProgressPreference='SilentlyContinue'\n"+script).encode('utf-16le')).decode()
    r=subprocess.run(['ssh','-o','BatchMode=yes','-o','ConnectTimeout=10',REMOTE,'powershell','-NoProfile','-EncodedCommand',encoded],capture_output=True,text=True,timeout=timeout,creationflags=subprocess.CREATE_NO_WINDOW|subprocess.BELOW_NORMAL_PRIORITY_CLASS)
    require(r.returncode == 0, 'Remote transfer control failed: '+r.stderr[-2000:])
    return r.stdout


def safe_remote(path):
    return isinstance(path,str) and path.startswith(ROOT+'/archives/') and all(c.isascii() and (c.isalnum() or c in '/:._-') for c in path) and '..' not in PurePosixPath(path).parts


def admission(plan, host):
    require(os.name=='nt' and host=='desktop' and plan['schema']=='g115-d4-archive-transfer/v1', 'Named Windows sender only')
    require(checked(plan['documents']['launcher']).resolve()==Path(__file__).resolve(), 'Wrong transfer owner')
    checked(plan['transport']['dispatcher'])
    require(plan['remote_root']==ROOT and plan['job_cap_bytes']==40_000_000_000 and plan['transfer_cap_bytes']==20_000_000_000 and plan['total_seconds']==1800, 'Fixed bounded transfer scope required')
    require(len(plan['archives'])==4 and len({r['destination'] for r in plan['archives']})==4, 'Four distinct declared ZIPs required')
    for ref in plan['archives']:
        require(safe_remote(ref['destination']) and ref['destination'].endswith('.zip') and ref['source']['sha256'] in ref['destination'], 'Transfer destination escaped named root')
        require(Path(ref['source']['path']).stat().st_size==ref['bytes'] and 0<ref['bytes']<9*1024**3, 'Archive source length changed')
    checked(plan['packet']['source'])
    require(plan['packet']['bytes']==Path(plan['packet']['source']['path']).stat().st_size and plan['packet']['bytes']<128*1024**2 and 0<plan['packet']['unpacked_bytes']<256*1024**2, 'Packet size differs')
    logical=sum(r['bytes'] for r in plan['archives'])+plan['packet']['bytes']+plan['packet']['unpacked_bytes']
    require(logical*1.15+32*1024**2<=plan['transfer_cap_bytes'], 'Transfer allocation exceeds cap')
    worker=Path(plan['worker_root'])
    require(worker.is_absolute() and worker.drive.lower()=='e:' and not worker.exists(), 'Fresh E transfer receipt tree required')
    live=json.loads(ssh("[pscustomobject]@{host=$env:COMPUTERNAME;exists=(Test-Path -LiteralPath '"+ROOT+"');free=(Get-Volume -DriveLetter C).SizeRemaining} | ConvertTo-Json"))
    require(live['host']=='COMPUTEHOST' and not live['exists'] and live['free']>=RESERVE+plan['job_cap_bytes'], 'Remote root exists or full job reserve unavailable')
    return live


def execute(plan, live):
    worker=Path(plan['worker_root']);worker.mkdir()
    started=time.monotonic();result={'complete':False,'inventory':live,'copies':[]}
    def guard():
        require(time.monotonic()-started<plan['total_seconds'], 'Transfer time bound reached')
        snapshot=json.loads(ssh("$r='"+ROOT+"';$files=@(Get-ChildItem -LiteralPath $r -Recurse -File);$n=0L;foreach($f in $files){$n+=$f.Length};[pscustomobject]@{bytes=$n;files=$files.Count;free=(Get-Volume -DriveLetter C).SizeRemaining} | ConvertTo-Json",timeout=30))
        require(snapshot['free']>=RESERVE and snapshot['bytes']*1.15+snapshot['files']*4096<=plan['transfer_cap_bytes'], 'Remote transfer cap/reserve reached')
        return snapshot
    def copy_file(source, destination, length, digest, number):
        require(safe_remote(destination), 'Unsafe transfer destination')
        parent=destination.rsplit('/',1)[0]
        ssh("if(Test-Path -LiteralPath '"+destination+"'){throw 'Destination already exists'}\nNew-Item -ItemType Directory -Force -Path '"+parent+"' | Out-Null")
        before=guard();require(before['free']>=RESERVE+length, 'Pending transfer would reach reserve')
        begin=time.monotonic();proc=None
        try:
            with (worker/f'copy-{number}.stdout').open('xb') as out,(worker/f'copy-{number}.stderr').open('xb') as err:
                proc=subprocess.Popen(['scp','-q','-o','BatchMode=yes','-o','ConnectTimeout=10',source,REMOTE+':'+destination],stdout=out,stderr=err,creationflags=subprocess.CREATE_NO_WINDOW|subprocess.BELOW_NORMAL_PRIORITY_CLASS)
                while True:
                    try:proc.wait(timeout=10);break
                    except subprocess.TimeoutExpired:
                        guard();require(Path(source).stat().st_size==length,'Source length changed while copying')
            require(proc.returncode==0,'Owned SCP failed; partial file retained')
        finally:
            if proc is not None and proc.poll() is None:proc.kill();proc.wait(timeout=30)
        verification=json.loads(ssh("$p='"+destination+"';[pscustomobject]@{bytes=(Get-Item -LiteralPath $p).Length;sha256=(Get-FileHash -LiteralPath $p).Hash.ToLowerInvariant()} | ConvertTo-Json"))
        require(verification=={'bytes':length,'sha256':digest},'Transferred file does not match declared source pin')
        row={'destination':destination,**verification,'seconds':time.monotonic()-begin}
        result['copies'].append(row);save(worker/f'copy-{number}.json',row);guard()
    try:
        ssh("New-Item -ItemType Directory -Path '"+ROOT+"/archives' | Out-Null")
        packet=plan['packet'];copy_file(packet['source']['path'],ROOT+'/archives/packet.zip',packet['bytes'],packet['source']['sha256'],0)
        unpack=ssh("$r='"+ROOT+"';Expand-Archive -LiteralPath ($r+'/archives/packet.zip') -DestinationPath ($r+'/packet');$h=Get-Content -LiteralPath ($r+'/packet/file-hashes.json') -Raw | ConvertFrom-Json;$n=0;foreach($e in $h.PSObject.Properties){if((Get-FileHash -LiteralPath ($r+'/packet/'+$e.Name)).Hash.ToLowerInvariant() -ne $e.Value){throw ('Packet hash differs '+$e.Name)};$n++};[pscustomobject]@{verified_files=$n} | ConvertTo-Json")
        result['packet_verification']=json.loads(unpack);guard()
        for i,ref in enumerate(plan['archives'],1):
            copy_file(ref['source']['path'],ref['destination'],ref['bytes'],ref['source']['sha256'],i)
        result['final_storage']=guard();result['complete']=True
    except BaseException as error:
        result['error']=repr(error);raise
    finally:
        result['seconds']=time.monotonic()-started;save(worker/'completion.json',result)


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--manifest',required=True);parser.add_argument('--host',required=True)
    parser.add_argument('--root');parser.add_argument('--check-only',action='store_true');args=parser.parse_args()
    plan=json.loads(Path(args.manifest).read_bytes());live=admission(plan,args.host)
    if args.check_only:print(json.dumps({'admitted':True,'native_started':False,'inventory':live}))
    else:
        require(args.root is not None and Path(args.root).resolve()==Path(plan['worker_root']).resolve(),'Transfer receipt root differs')
        execute(plan,live)
