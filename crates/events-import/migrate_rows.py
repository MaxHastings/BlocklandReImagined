"""Offline BLS +-EVENT rows to native rows. Preserves row indices and unsupported text."""
import json,sys,math
from pathlib import Path
SLOTS={'self':'SelfBrick','player':'Player','client':'Client','projectile':'Projectile','bot':'Bot','driver':'Driver','minigame':'MiniGame','ball':'Ball'}
def parse_value(p,text,aliases):
    kind=p['type']
    if kind in ('int','list'):
        n=int(text or p.get('default',p.get('items',[['',0]])[0][1]));assert kind!='int' or p['min']<=n<=p['max'];assert kind!='list' or n in [v for _,v in p['items']];return {'Int':n}
    if kind=='float':
        n=float(text or p['default']);assert math.isfinite(n) and p['min']<=n<=p['max'];n=p['min']+math.floor((n-p['min'])/p['step']+1e-5)*p['step'];return {'Float':n}
    if kind=='bool':assert text in ('0','1','');return {'Bool':text=='1'}
    if kind=='string':assert len(text)<=p['max_length'];return {'Text':text}
    if kind=='paint_color':n=int(text or p['default']);assert 0<=n<256;return {'Color':n}
    if kind=='int_list':
        if text.upper()=='ALL':return {'Rows':'All'}
        nums=[int(n) for n in text.split()];assert len(nums)<=4096 and all(0<=n<4096 for n in nums);return {'Rows':{'Indices':nums}}
    if kind=='vector':
        a=[float(x) for x in (text or '0 0 0').split()];assert len(a)==3 and all(math.isfinite(x) for x in a) and sum(x*x for x in a)<=p['max_length']**2+1e-5;return {'Vector':[a[0],a[2],-a[1]]}
    if kind=='datablock':
        if text in ('','-1','0'):return {'Datablock':None}
        choices=aliases.get(p['class_name'],{});hits=[v for k,v in choices.items() if k.lower()==text.lower()];assert len(hits)==1,f'unresolved {p["class_name"]} {text}';return {'Datablock':hits[0]}
    raise ValueError(kind)
def parse_row(line,catalog,aliases):
    f=line.rstrip('\r\n').split('\t');assert len(f)==12 and f[0]=='+-EVENT'
    assert f[2] in ('0','1');delay=int(f[4]);assert 0<=delay<=30000
    inp=next(x for x in catalog['inputs'] if x['name'].lower()==f[3].lower())
    if f[5] in ('-1','<NAMED BRICK>'):target={'Named':f[6]};cls='fxDTSBrick'
    else:
        cls=next(c for t,c in inp['targets'] if t.lower()==f[5].lower());target={'Slot':SLOTS[f[5].lower()]}
    out=next(x for x in catalog['outputs'] if x['name'].lower()==f[7].lower() and x['class_name'].lower()==cls.lower())
    assert not any(f[8+len(out['params']):]),'unexpected extra parameters'
    return dict(preserved=None,enabled=f[2]=='1',input=inp['name'],delay_ms=delay,target=target,output=out['name'],params=[parse_value(p,v,aliases) for p,v in zip(out['params'],f[8:])])
def preserved(line,error):return dict(preserved=dict(original=line,diagnostic=error[:1024]),enabled=True,input='',delay_ms=0,target={'Slot':'SelfBrick'},output='',params=[])
def migrate(lines,catalog,aliases):
    rows={};diagnostics=[]
    for line in lines:
        if not line.startswith('+-EVENT\t'):continue
        idx=int(line.split('\t')[1]);assert 0<=idx<4096 and idx not in rows,'duplicate/oversized source row index'
        try:rows[idx]=parse_row(line,catalog,aliases)
        except (ValueError,AssertionError,StopIteration,KeyError) as e:rows[idx]=preserved(line,str(e) or 'unregistered event/target');diagnostics.append(dict(row=idx,reason=rows[idx]['preserved']['diagnostic']))
    out=[rows.get(i,preserved('','missing original row index; no execution')) for i in range(max(rows,default=-1)+1)]
    return dict(schema_version=1,rows=out,diagnostics=diagnostics,runnable=sum(x['preserved'] is None for x in out),preserved=sum(x['preserved'] is not None for x in out))
def main():
    reference,catalog,rows,aliases,out=map(Path,sys.argv[1:]);out=out.resolve();assert not out.exists() and not out.is_relative_to(reference.resolve())
    assert rows.stat().st_size<=64<<20 and catalog.stat().st_size<=4<<20 and aliases.stat().st_size<=8<<20
    result=migrate(rows.read_text(encoding='utf-8-sig').splitlines(),json.loads(catalog.read_text()),json.loads(aliases.read_text()))
    out.write_text(json.dumps(result,indent=2)+'\n');print(json.dumps({k:v for k,v in result.items() if k not in ('rows','diagnostics')}))
if __name__=='__main__':main()
