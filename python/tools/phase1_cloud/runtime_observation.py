"""One actual owned-child Linux mapping sample, never a parity substitute."""
from __future__ import annotations
import copy
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import uuid
from common import pin, require

IMAGE_REFERENCE='docker.io/library/python@sha256:9c47360a2a0355e2da18516d0b1c2126ec22c195d2185e97347c9d98398c5bef'
LIBRARIES=('ld-linux-x86-64.so.2','libc.so.6','libm.so.6','libgcc_s.so.1')
ENVIRONMENT={'PATH':'/usr/local/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin',
    'LANG':'C.UTF-8','LC_ALL':'C.UTF-8','CUDA_VISIBLE_DEVICES':'',
    'OMP_NUM_THREADS':'1','MKL_NUM_THREADS':'1','OPENBLAS_NUM_THREADS':'1'}


def controlled_environment(inherited):
    for name,value in inherited.items():
        require(not (value and (name.startswith('LD_') or name=='GLIBC_TUNABLES')),
                'unqualified native runtime environment override: '+name)
    # The native program needs only these deterministic runtime controls. Do not
    # copy provider credentials or unrelated account/application environment.
    return dict(ENVIRONMENT)


def verify_environment(value):
    require(isinstance(value,dict),'native environment observation unavailable')
    require(all(value.get(key)==expected for key,expected in ENVIRONMENT.items()),
            'native controlled environment differs')
    extras=set(value)-set(ENVIRONMENT)
    require(all((key.startswith('LD_') or key=='GLIBC_TUNABLES') and value[key] in ('',None)
                for key in extras),'native environment contains unqualified or non-whitelisted values')
    return dict(ENVIRONMENT)


def image_contract(image):
    require(image.get('schema')=='phase1-runtime-image-pins-v1' and
            image.get('platform_reference')==IMAGE_REFERENCE,'accepted common Linux image descriptor required')
    libraries={}
    for row in image['runtime_files']:
        name=PurePosixPath(row['resolved_path']).name
        if name not in LIBRARIES:continue
        require(name not in libraries and PurePosixPath(row['resolved_path']).is_absolute() and
                '..' not in PurePosixPath(row['resolved_path']).parts and
                re.fullmatch('[0-9a-f]{64}',row['sha256']) and
                type(row['bytes']) is int and row['bytes']>0,'ambiguous or incomplete pinned runtime library')
        libraries[name]=row
    require(set(libraries)==set(LIBRARIES),'loader/libc/libm/libgcc pins required')
    return libraries


class ProcReader:
    def text(self,pid,name):return Path(f'/proc/{pid}/{name}').read_text()
    def executable(self,pid):return str(Path(f'/proc/{pid}/exe').resolve(strict=True))
    def file(self,path):
        value=pin(path);stat=Path(path).stat()
        return value|{'inode':stat.st_ino,'device_major':os.major(stat.st_dev),'device_minor':os.minor(stat.st_dev)}
    def host(self):
        fields={}
        for line in Path('/proc/cpuinfo').read_text().split('\n\n')[0].splitlines():
            if ':' in line:
                key,value=line.split(':',1);fields[key.strip()]=value.strip()
        require(fields.get('model name') and fields.get('flags'),'CPU observations unavailable')
        return {'cpu_model':fields['model name'],'cpu_features':sorted(set(fields['flags'].split())),
                'kernel_release':os.uname().release}


def starttime(text):return int(text.rsplit(') ',1)[1].split()[19])


def sample(child,binary,image,environment,quota_context,system=None):
    system=system or ProcReader();expected=image_contract(image)
    verify_environment(environment)
    require(isinstance(quota_context,dict) and quota_context.get('affinity') and
            isinstance(quota_context.get('quota'),dict),'actual child quota context required')
    try:
        before=starttime(system.text(child.pid,'stat'))
        executable=system.executable(child.pid)
        if executable!=str(Path(binary['path']).resolve()):return None
        raw=system.text(child.pid,'maps');observed={};mapped={};actual_binary=None
        for line in raw.splitlines():
            parts=line.split(maxsplit=5)
            if len(parts)!=6:continue
            deleted=parts[5].endswith(' (deleted)');path=parts[5].removesuffix(' (deleted)')
            name=PurePosixPath(path).name
            is_binary=path==executable
            if name not in expected and not is_binary:continue
            require(not deleted,'required mapped executable/library was deleted')
            if not is_binary:require(path==expected[name]['resolved_path'],'mapped runtime library path differs')
            if path in mapped:continue
            actual=system.file(path);major,minor=(int(v,16) for v in parts[3].split(':'))
            require((actual['inode'],actual['device_major'],actual['device_minor'])==(int(parts[4]),major,minor),
                    'mapped executable/library file identity differs')
            target=binary if is_binary else expected[name]
            require(actual['sha256']==target['sha256'] and
                    ('bytes' not in target or actual['bytes']==target['bytes']),
                    'mapped executable/library bytes differ from pinned inputs')
            file_pin={key:actual[key] for key in ('path','sha256','bytes')};mapped[path]=actual
            if is_binary:actual_binary=file_pin
            else:observed[name]=file_pin
        if actual_binary is None or set(observed)!=set(LIBRARIES):return None
        require(before==starttime(system.text(child.pid,'stat')) and executable==system.executable(child.pid),
                'owned process identity changed during runtime observation')
        return {'schema':'phase1-sampled-linux-runtime/v2','pid':child.pid,'proc_starttime_ticks':before,
            'executable':actual_binary,'libraries':observed,'mapped_files':mapped,'maps':raw,
            'image_reference':IMAGE_REFERENCE,'environment':dict(environment),**system.host(),
            'quota_context':copy.deepcopy(quota_context),
            'limits':['One sample of the owned process mappings; not continuous attestation or numerical parity',
                      'Quota context reports the visible hierarchy and any missing or hidden limits without inferring entitlement']}
    except (FileNotFoundError,ProcessLookupError):return None


def sync_directory(path):
    """Linux directory entry durability; unsupported/error cases must propagate."""
    descriptor=os.open(path,os.O_RDONLY|os.O_DIRECTORY)
    try:os.fsync(descriptor)
    finally:os.close(descriptor)


def capture(sampled,durable):
    """Retain mapped bytes for read-only verification after the Pod is released."""
    value=copy.deepcopy(sampled);captured={}
    for source in [sampled['executable'],*sampled['libraries'].values()]:
        pin(source['path'],source['sha256'])
        destination=Path(durable)/'runtime-files'/source['sha256']/Path(source['path']).name
        destination.parent.mkdir(parents=True,exist_ok=True)
        if not destination.exists():
            temporary=destination.with_name(destination.name+'.'+uuid.uuid4().hex+'.partial')
            with Path(source['path']).open('rb') as inp,temporary.open('xb') as out:
                shutil.copyfileobj(inp,out,1048576);out.flush();os.fsync(out.fileno())
            pin(temporary,source['sha256']);os.replace(temporary,destination)
        # Persist both the file rename and any newly created digest/runtime-files
        # directory entries. A failed fsync prevents a verified runtime receipt.
        for directory in (destination.parent,destination.parent.parent,Path(durable)):
            sync_directory(directory)
        captured[source['path']]=pin(destination,source['sha256'])
    value['captured_files']=captured
    return value
