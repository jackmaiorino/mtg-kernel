"""Package complete public recordings and verify immutable release archives."""
import argparse
import hashlib
import json
import zipfile
from pathlib import Path, PurePosixPath, PureWindowsPath

def sha(path): return hashlib.sha256(path.read_bytes()).hexdigest()

def safe_path(root: Path, name: str) -> Path:
    if not isinstance(name, str) or not name or '\\' in name or PureWindowsPath(name).drive:
        raise ValueError('Unsafe release path')
    p=PurePosixPath(name)
    if p.is_absolute() or '..' in p.parts:
        raise ValueError('Unsafe release path')
    result=(root/name).resolve()
    if not result.is_relative_to(root.resolve()):
        raise ValueError('Release path escapes its root')
    return result

def pack(root, output):
    manifest = json.loads((root/'manifest.json').read_text())
    if not manifest.get('games'): raise ValueError('Demo manifest has no games')
    members = {'manifest.json'}
    for game in manifest['games']:
        path = safe_path(root, game['file'])
        if sha(path) != game['sha256']: raise ValueError('Recording hash differs')
        r = json.loads(path.read_text(encoding='utf-8'))
        if not r['complete'] or not r['frames'][-1]['terminal'] or len(r['bookmarks']) != 3: raise ValueError('Release requires a complete game and three guided moments')
        members.add(game['file'])
        for card in r['cards'].values():
            if card['image']: members.add(card['image'])
    with zipfile.ZipFile(output,'w',zipfile.ZIP_DEFLATED,compresslevel=9) as z:
        for name in sorted(members):
            z.write(safe_path(root,name),name)
    print(json.dumps({'sha256':sha(output),'bytes':output.stat().st_size,'members':len(members)}))

def unpack(archive,destination,release):
    pins=json.loads(release.read_text())
    if sha(archive) != pins['sha256']: raise ValueError('Release archive hash mismatch')
    destination.mkdir(parents=True,exist_ok=True)
    with zipfile.ZipFile(archive) as z:
        if sum(i.file_size for i in z.infolist()) > 256*1024**2: raise ValueError('Release pack too large')
        for item in z.infolist():
            if item.is_dir(): raise ValueError('Unexpected directory member')
            path=safe_path(destination,item.filename)
            path.parent.mkdir(parents=True,exist_ok=True)
            path.write_bytes(z.read(item))
    manifest=json.loads((destination/'manifest.json').read_text())
    for game in manifest['games']:
        if sha(safe_path(destination,game['file'])) != game['sha256']: raise ValueError('Materialized recording hash mismatch')

def main():
    ap=argparse.ArgumentParser(description=__doc__)
    ap.add_argument('mode',choices=('pack','unpack'));ap.add_argument('source',type=Path);ap.add_argument('target',type=Path);ap.add_argument('--release',type=Path)
    a=ap.parse_args()
    if a.mode=='pack':pack(a.source,a.target)
    else:
        if not a.release:ap.error('--release is required for unpack')
        unpack(a.source,a.target,a.release)
if __name__=='__main__':main()
