"""Offline literal registration compiler. Never executes Torque or modifies inputs."""
import hashlib,json,re,sys,zipfile
from pathlib import Path

def sha(b):return hashlib.sha256(b).hexdigest()
def strings(s):
    q=re.findall(r'"([^"\\]*(?:\\.[^"\\]*)*)"',s)
    rest=re.sub(r'"([^"\\]*(?:\\.[^"\\]*)*)"','',s).strip()
    assert re.fullmatch(r'(?:\s*TAB\s*)*',rest),repr(s)
    return q

def splitargs(s):return re.split(r',(?=(?:[^"\n]*"[^"\n]*")*[^"\n]*$)',s.replace('\r','').replace('\n',' '))
def spec(s):
    t=s.split();kind=t.pop(0).lower()
    if kind=='int':return dict(type='int',min=int(t[0]),max=int(t[1]),default=int(t[2]))
    if kind=='float':return dict(type='float',min=float(t[0]),max=float(t[1]),step=float(t[2]),default=float(t[3]))
    if kind=='bool':return dict(type='bool')
    if kind=='vector':return dict(type='vector',max_length=float(t[0]))
    if kind=='paintcolor':return dict(type='paint_color',default=int(t[0]))
    if kind=='intlist':return dict(type='int_list',width=int(t[0]))
    if kind=='string':return dict(type='string',max_length=int(t[0]),width=int(t[1]))
    if kind=='datablock':return dict(type='datablock',class_name=t[0])
    if kind=='list':return dict(type='list',items=[[t[i],int(t[i+1])] for i in range(0,len(t),2)])
    raise ValueError(s)

def main():
    root,core,out=map(Path,sys.argv[1:]);root=root.resolve();out=out.resolve()
    assert not out.exists() and not out.is_relative_to(root),'fresh output outside source required'
    out.parent.mkdir(parents=True,exist_ok=True)
    scripts=[('recovered/core/allGameScripts-Vanilla.cs',core.read_bytes(),'recovered_core')]
    packages=[]
    for archive in sorted((root/'Add-Ons').glob('*.zip')):
        assert archive.resolve().is_relative_to(root)
        with zipfile.ZipFile(archive) as z:
            assert len(z.namelist())<=65536
            for n in z.namelist():
                if not n.lower().endswith('.cs'):continue
                assert z.getinfo(n).file_size<=8<<20
                b=z.read(n)
                if re.search(rb'register(?:Input|Output)Event\s*\(',b,re.I):
                    scripts.append((f'Add-Ons/{archive.stem}/{n}',b,'primary_addon'))
                    packages.append(archive.stem)
    catalog=dict(schema_version=1,inputs=[],outputs=[],sources=[],scope={'packages_with_registrations':sorted(set(packages)),'scanned_packages':sorted(p.stem for p in (root/'Add-Ons').glob('*.zip')),'note':'Core plus registration-bearing packages in designated 79-package reference. Tutorial input is map-specific; registration dispatch does not establish its host binding.'})
    proof={}
    for path,b,kind in scripts:
        assert len(b)<=8<<20
        text=b.decode('utf-8-sig');digest=sha(b);proof[digest+'.source']=b
        catalog['sources'].append(dict(path=path,sha256=digest,bytes=len(b),copy_file=digest+'.source',kind=kind))
        # Quoted literal calls only, including multiline stock addon registrations.
        for m in re.finditer(r'(?im)^\s*register(Input|Output)Event\s*\(([^;]+)\)\s*;',text):
            args=splitargs(m[2]);cls=strings(args[0])[0];name=strings(args[1])[0]
            d=dict(id=f'v20/event/{cls.lower()}/{name.lower()}',class_name=cls,name=name,source=path,source_line=text.count('\n',0,m.start()+len(m[0])-len(m[0].lstrip()))+1)
            if m[1].lower()=='input':
                d['targets']=[v.split() for v in strings(args[2])];catalog['inputs'].append(d)
            else:
                params=strings(args[2]);d.update(params=[spec(x) for x in params if x],append_client=len(args)<4 or int(args[3])!=0);catalog['outputs'].append(d)
    assert len(catalog['inputs'])==16
    for key in ['inputs','outputs']:
        assert len({d['id'] for d in catalog[key]})==len(catalog[key]);catalog[key].sort(key=lambda d:d['id'])
    out.mkdir()
    for name,b in proof.items():(out/name).write_bytes(b)
    (out/'catalog.json').write_text(json.dumps(catalog,indent=2)+'\n',encoding='utf-8')
    print(json.dumps({'inputs':len(catalog['inputs']),'outputs':len(catalog['outputs']),'sources':len(catalog['sources']),'packages':sorted(set(packages))},indent=2))
if __name__=='__main__':main()
