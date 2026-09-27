"""Independent source hashes + registration identities + native UI table comparison."""
import hashlib,json,re,sys,zipfile
from pathlib import Path
pack,reference,core,ui,out=map(Path,sys.argv[1:]);assert not out.resolve().is_relative_to(reference.resolve());assert (pack/'catalog.json').stat().st_size<=4<<20;c=json.loads((pack/'catalog.json').read_text());assert c['schema_version']==1
identities={'inputs':set(),'outputs':set()}
for s in c['sources']:
 path=(pack/s['copy_file']).resolve();assert path.is_relative_to(pack.resolve()) and path.stat().st_size<=8<<20
 b=path.read_bytes();assert len(b)==s['bytes'] and hashlib.sha256(b).hexdigest()==s['sha256']
 if s['kind']=='recovered_core':original=core.read_bytes()
 else:
  _,package,member=s['path'].split('/',2)
  with zipfile.ZipFile(reference/'Add-Ons'/(package+'.zip')) as z:original=z.read(member)
 assert original==b,s['path']
 text=b.decode('utf-8-sig')
 for kind,cls,name in re.findall(r'register(Input|Output)Event\s*\(\s*"([^"]+)"\s*,\s*"([^"]+)"',text):identities['inputs' if kind=='Input' else 'outputs'].add((cls.lower(),name.lower()))
 for definition in c['inputs']+c['outputs']:
  if definition['source']==s['path']:assert text.splitlines()[definition['source_line']-1].lstrip().startswith('register'),definition
for k in identities:assert identities[k]=={(d['class_name'].lower(),d['name'].lower()) for d in c[k]}
u=json.loads(ui.read_text())['data']['event_tables'];outs={(d['class_name'].lower(),d['name'].lower()):d for d in c['outputs']}
for d in u['outputs']:
 n=outs[(d['class'].lower(),d['name'].lower())];params=[]
 for p in d['params']:
  p=p.copy()
  if p['type']=='datablock':p['class_name']=p.pop('class')
  if p['type']=='vector':p['max_length']=p.pop('max')
  params.append(p)
 assert params==n['params'] and d['append_client']==n['append_client'],d['name']
assert len(c['inputs'])==16 and len(c['outputs'])==65 and len(u['outputs'])==65
report={'passed':True,'inputs':16,'outputs':65,'source_records':len(c['sources']),'source_bytes_identical':True,'source_line_numbers_verified':True,'ui_outputs_equal':65,'ui_core_inputs':len(u['inputs']),'additional_inputs':sorted(identities['inputs']-{(d['class'].lower(),d['name'].lower()) for d in u['inputs']}),'scanned_packages':len(c['scope']['scanned_packages']),'sha256':hashlib.sha256((pack/'catalog.json').read_bytes()).hexdigest()}
out.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
