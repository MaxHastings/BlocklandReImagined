"""Stage a private audible preview from verified v041 content, without builds.

Never launches a visible game. --finalize hashes the supplied warm executables
and performs the packaged headless startup check against isolated state.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

import build_map_ambience
from map_ambience.approvals import original_map_problems
from map_ambience.original_policy import apply as apply_original_policy

REPO=Path(__file__).resolve().parent.parent


def sha(path):
    with path.open('rb') as f:
        return hashlib.file_digest(f,'sha256').hexdigest()


def stage(seed,output,generated,importer,originals,settings):
    seed=seed.resolve();output=output.resolve()
    if output.exists() or output==seed or output.is_relative_to(seed):
        raise ValueError('preview needs a new separate output directory')
    bad=build_map_ambience.problems(REPO,generated)
    if bad: raise ValueError('; '.join(bad))
    generated_receipt=json.loads((generated/'map-ambience-generated.json').read_text())
    source_hashes={r['input_sha256'] for r in generated_receipt['clips']}
    manifest=json.loads((seed/'MANIFEST.json').read_text())
    if manifest['version']!='v0.4.1': raise ValueError('use the clean published v041 seed')
    output.mkdir(parents=True)
    copied=[]
    for row in manifest['files']:
        relative=Path(row['path'])
        if relative.is_absolute() or '..' in relative.parts: raise ValueError('unsafe seed inventory')
        if not (row['path'].startswith(('content/','licenses/')) or row['path'] in ['dxcompiler.dll','dxil.dll','CREDITS.md']):
            continue
        source=seed/relative
        if sha(source)!=row['sha256']: raise ValueError('seed differs from published manifest: '+row['path'])
        target=output/relative;target.parent.mkdir(parents=True,exist_ok=True)
        shutil.copy2(source,target);copied.append(row)
    for name in ['map_ambience','map_ambience_rules']:
        shutil.copytree(generated.parent/name,output/'content/addons'/name)
    refreshed=[]
    for name,archive in [('map_creek','Map_Creek.zip'),('map_forest','Map_Forest.zip')]:
        target=output/'content/addons'/name
        subprocess.run([str(importer),'map-audio',str(originals/archive),str(target)],check=True)
        override=apply_original_policy(REPO,target)
        bad=original_map_problems(REPO,target)
        if bad: raise ValueError('; '.join(bad))
        receipt=json.loads((target/'map-audio-provenance.json').read_text())
        enabled=set(override['enabled_profiles']) if override else {r['profile'] for r in receipt['sources']}
        source_hashes.update(r['sha256'] for r in receipt['sources'] if r['profile'] in enabled)
        if len(receipt['sources']) != (3 if name=='map_creek' else 1):
            raise ValueError('original emitter inventory differs from approved4')
        refreshed.append(json.loads((target/'map-audio-refresh.json').read_text()))
    list_path=output/'content/packages.json';packages=json.loads(list_path.read_text())
    additions=[('map_ambience','shared'),('map_ambience_rules','server'),
               ('map_creek_ambience_rules','server'),('map_forest_ambience_rules','server')]
    for name,side in additions:
        if any(p['id']==name for p in packages['packages']): raise ValueError('duplicate preview package')
        m=json.loads((output/'content/addons'/name/'package.json').read_text())
        packages['packages'].append({'id':name,'version':m['version'],'side':side,'dir':'addons/'+name})
    list_path.write_text(json.dumps(packages,indent=2)+'\n')
    state=output/'state';state.mkdir()
    if settings.is_file(): shutil.copy2(settings,state/'settings.json')
    (output/'Launch-Ambience.cmd').write_text('@echo off\nsetlocal\nset BRI_INSTALL_DEFAULT_ADD_ONS=0\ncd /d "%~dp0"\n"%~dp0bri-client.exe" --run "%~dp0content" "%~dp0state"\n')
    (output/'README-AMBience.txt').write_text(
        'Private ambience preview; not merged/released v0.4.2. Start Launch-Ambience.cmd.\n'
        f'{len(source_hashes)} approved starting recordings. Added palette covers21maps; Arctic remains installed/off as before.\n'
        'Your indoor/outdoor bases cover all20 nonstorm maps, including player-built interiors. Storm retains its existing mix.\n'
        'Only localized Kitchen fridge, Creek waterfall and shoreline waves are additional active ambience.\n'
        'Other birds/frogs/crickets/old global bed variations are preserved inactive. Final combined game mix acceptance remains pending.\n'
        'Compare normal footsteps/tools/weapons/NPC/voice with outdoors, roof-only, sealed room and an opening.\n'
        'Waterfall retains its original loud level initially: specifically report masking near the waterfall.\n'
        'State/settings are isolated here; only a copy of existing settings was used.\n')
    with (output/'CREDITS.md').open('a',encoding='utf-8') as f:
        f.write('\nPrivate ambience additions: content/addons/map_ambience/map-ambience-generated.json. '
                'Original Creek/Forest audio retains map author provenance; see map-audio-provenance.json in those packages.\n')
    receipt={'schema_version':1,'seed':'v0.4.1','seed_manifest_sha256':sha(seed/'MANIFEST.json'),
             'tool_source':subprocess.check_output(['git','rev-parse','HEAD'],cwd=REPO,text=True).strip(),
             'approved_recordings':len(source_hashes),'palette_maps':21,
             'palette_profiles':len(generated_receipt['clips']),'seed_files_verified':len(copied),
             'original_refreshes':refreshed,'generated_receipt_sha256':sha(generated/'map-ambience-generated.json'),
             'mix_acceptance':'two user bases and full prepared loop seams approved; combined in-game listening and release blessing pending',
             'note':'Whole-import geometry generation identity retained; private audio overlays are separately receipted.'}
    (output/'ambience-preview.json').write_text(json.dumps(receipt,indent=2)+'\n')
    print('Private content staged:',output,'; warm executables still required')


def finalize(output,expected_version):
    receipt=json.loads((output/'ambience-preview.json').read_text())
    bad=build_map_ambience.problems(REPO,output/'content/addons/map_ambience')
    if bad:raise ValueError('; '.join(bad))
    for name in ['map_creek','map_forest']:
        bad=original_map_problems(REPO,output/'content/addons'/name)
        if bad:raise ValueError('; '.join(bad))
    exes=['bri-client.exe','bri-import-addon.exe','bri-addon-check.exe','bri-addon-run.exe','bri-server.exe']
    for name in exes:
        if not (output/name).is_file(): raise ValueError('missing warm executable: '+name)
    version=subprocess.check_output([str(output/'bri-client.exe'),'--version'],text=True).strip()
    if expected_version not in version: raise ValueError('wrong warm client version: '+version)
    receipt['client_version']=version;receipt['executables']={n:sha(output/n) for n in exes}
    for name in ['map_ambience','map_ambience_rules','map_creek_ambience_rules','map_forest_ambience_rules']:
        subprocess.run([str(output/'bri-addon-check.exe'),str(output/'content/addons'/name),'--json'],check=True)
    subprocess.run([str(output/'bri-client.exe'),'--check',str(output/'content'),str(output/'state')],check=True)
    receipt['packaged_headless_startup']='passed; no window/audio device opened'
    (output/'ambience-preview.json').write_text(json.dumps(receipt,indent=2)+'\n')
    files=[{'path':p.relative_to(output).as_posix(),'sha256':sha(p),'bytes':p.stat().st_size}
           for p in sorted(output.rglob('*')) if p.is_file() and not p.relative_to(output).parts[0] in {'state','logs'} and p.name!='PREVIEW-MANIFEST.json']
    (output/'PREVIEW-MANIFEST.json').write_text(json.dumps({'schema_version':1,'files':files},indent=2)+'\n')
    print('Verified playable private preview:',output)


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--seed',type=Path,default=Path('E:/Downloads/BlocklandReImagined-v0.4.1-windows'))
    p.add_argument('--output',type=Path,required=True)
    p.add_argument('--generated',type=Path)
    p.add_argument('--importer',type=Path)
    p.add_argument('--originals',type=Path,default=Path('S:/SteamLibrary/steamapps/common/Blockland/Add-Ons'))
    p.add_argument('--settings',type=Path,default=Path('C:/Users/Maxwell/AppData/Local/BlocklandReImagined/settings.json'))
    p.add_argument('--finalize',action='store_true')
    p.add_argument('--expected-version',default='v0.4.2-ambience-preview1')
    a=p.parse_args()
    if a.finalize: finalize(a.output.resolve(),a.expected_version)
    else:
        if not a.generated or not a.importer: p.error('staging requires --generated and --importer')
        stage(a.seed,a.output,a.generated.resolve(),a.importer.resolve(),a.originals.resolve(),a.settings)
