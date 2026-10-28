"""Make an offline, one-second movement/decision timeline from a bot match report.

python tools/render_bot_match.py artifacts/match.json artifacts/match.html
The HTML embeds private match evidence. Keep generated files out of Git.
Symbols show observed positions, not exact collision hulls or a 3D game replay.
"""

import argparse
import json
from pathlib import Path


HTML = r'''<!doctype html><meta charset="utf-8"><title>Bot match evidence</title>
<style>
body{margin:0;background:#171a20;color:#eee;font:14px system-ui}header{padding:12px;display:flex;gap:12px;align-items:center;flex-wrap:wrap}
input[type=range]{flex:1;min-width:220px}button,select{padding:7px;background:#313742;color:white;border:1px solid #657084;border-radius:4px}
main{display:grid;grid-template-columns: minmax(400px,1fr) 340px;gap:12px;padding:0 12px}canvas{width:100%;height:76vh;background:#242830}
aside{overflow:auto;height:76vh}pre{white-space:pre-wrap;word-break:break-word;font:12px ui-monospace,monospace}small{display:block;padding:12px;color:#bcc4d1}
</style>
<header><b id="title"></b><button id="play">Play</button><select id="speed"><option>1</option><option selected>5</option><option>10</option><option>20</option></select>×
<input id="time" type="range" min="0" value="0"><span id="stamp"></span><select id="bot"><option value="">All bots</option></select></header>
<main><canvas id="map"></canvas><aside><b>Selected bot / outcomes</b><pre id="detail"></pre></aside></main>
<small>Observed positions at 1 Hz. Solid trails: previous 15 seconds. Dotted lines: selected bot's goal and next waypoint. Diamonds: vehicles; circles: bots; faded symbols: dead bodies. Explicit goofs have a gold ring. No controls or bot goals were injected.</small>
<script id="evidence" type="application/json">DATA</script>
<script>
const data=JSON.parse(document.getElementById('evidence').textContent), $=id=>document.getElementById(id);
const frames=new Map(), ids=new Set(), vehicles=new Map();
for(const b of data.bots){if(!frames.has(b.second))frames.set(b.second,[]);frames.get(b.second).push(b);ids.add(b.id)}
for(const v of data.vehicles){if(!vehicles.has(v.second))vehicles.set(v.second,[]);vehicles.get(v.second).push(v)}
$('title').textContent=data.world+' · '+data.seconds+' simulated seconds';$('time').max=data.seconds-1;
for(const id of [...ids].sort((a,b)=>a-b)){const o=document.createElement('option');o.value=id;o.textContent='Bot '+id;$('bot').appendChild(o)}
const points=data.bots.filter(b=>b.position).map(b=>b.position), xs=points.map(p=>p[0]), zs=points.map(p=>p[2]);
const bounds=[Math.min(...xs)-8,Math.max(...xs)+8,Math.min(...zs)-8,Math.max(...zs)+8];
const canvas=$('map'), ctx=canvas.getContext('2d');let running=false, last=0, elapsed=0;
function color(b){return b.team===1?'#75b8ff':b.team===2?'#ffad75':'#b9a8ff'}
function draw(){
 const second=+$('time').value, selected=+$('bot').value, bots=frames.get(second)||[];
 const ratio=devicePixelRatio||1;canvas.width=canvas.clientWidth*ratio;canvas.height=canvas.clientHeight*ratio;ctx.scale(ratio,ratio);
 const w=canvas.clientWidth,h=canvas.clientHeight,scale=Math.min((w-40)/(bounds[1]-bounds[0]),(h-40)/(bounds[3]-bounds[2]));
 const xoff=(w-(bounds[1]-bounds[0])*scale)/2,zoff=(h-(bounds[3]-bounds[2])*scale)/2;
 const xy=p=>[xoff+(p[0]-bounds[0])*scale,zoff+(p[2]-bounds[2])*scale];
 ctx.lineWidth=1;ctx.strokeStyle='#383e49';ctx.fillStyle='#8c96a7';ctx.font='10px system-ui';
 for(let x=Math.ceil(bounds[0]/20)*20;x<=bounds[1];x+=20){const a=xy([x,0,bounds[2]]),b=xy([x,0,bounds[3]]);ctx.beginPath();ctx.moveTo(...a);ctx.lineTo(...b);ctx.stroke();ctx.fillText('x '+x,a[0]+2,12)}
 for(let z=Math.ceil(bounds[2]/20)*20;z<=bounds[3];z+=20){const a=xy([bounds[0],0,z]),b=xy([bounds[1],0,z]);ctx.beginPath();ctx.moveTo(...a);ctx.lineTo(...b);ctx.stroke();ctx.fillText('z '+z,3,a[1]-2)}
 for(const b of bots){
  if(!b.position)continue;ctx.globalAlpha=selected&&selected!==b.id?.18:.65;ctx.strokeStyle=color(b);ctx.lineWidth=selected===b.id?2:1;
  ctx.beginPath();let prior=null;for(let t=Math.max(0,second-15);t<=second;t++){const old=(frames.get(t)||[]).find(o=>o.id===b.id);if(!old?.position)continue;const p=xy(old.position);if(!prior||old.spawn!==prior.spawn)ctx.moveTo(...p);else ctx.lineTo(...p);prior=old}ctx.stroke();
 }
 ctx.globalAlpha=1;
 for(const v of vehicles.get(second)||[]){const p=xy(v.position);ctx.strokeStyle='#c9d1dc';ctx.lineWidth=2;ctx.beginPath();ctx.moveTo(p[0],p[1]-6);ctx.lineTo(p[0]+6,p[1]);ctx.lineTo(p[0],p[1]+6);ctx.lineTo(p[0]-6,p[1]);ctx.closePath();ctx.stroke();ctx.fillStyle='#eee';ctx.fillText('V'+v.id,p[0]+8,p[1]-5)}
 for(const b of bots){if(!b.position)continue;const p=xy(b.position);ctx.globalAlpha=!b.alive?.3:selected&&selected!==b.id?.25:1;ctx.fillStyle=color(b);ctx.beginPath();ctx.arc(...p,b.id===selected?5:3.5,0,Math.PI*2);ctx.fill();
  if(b.goof){ctx.strokeStyle='#ffd96b';ctx.beginPath();ctx.arc(...p,8,0,Math.PI*2);ctx.stroke()}ctx.fillStyle='#eee';ctx.fillText(b.id,p[0]+5,p[1]+4);
  if(b.id===selected){ctx.setLineDash([4,4]);for(const [target,c] of [[b.goal,'#e7dc88'],[b.next,'#70e7af']]){if(!target)continue;ctx.strokeStyle=c;ctx.beginPath();ctx.moveTo(...p);ctx.lineTo(...xy(target));ctx.stroke()}ctx.setLineDash([])}
 }
 ctx.globalAlpha=1;$('stamp').textContent=Math.floor(second/60)+':'+String(second%60).padStart(2,'0');
 const b=bots.find(b=>b.id===selected);const recent=data.rounds.filter(r=>r.second<=second);
 $('detail').textContent=JSON.stringify(b||{bots:bots.map(b=>({id:b.id,team:b.team,behaviour:b.behaviour,weapon:b.weapon,goof:b.goof})),rounds:recent},null,2);
}
$('time').oninput=draw;$('bot').onchange=draw;window.onresize=draw;
$('play').onclick=()=>{running=!running;$('play').textContent=running?'Pause':'Play'};
function animate(now){if(running&&last){elapsed+=(now-last)/1000*(+$('speed').value);if(elapsed>=1){$('time').value=Math.min(data.seconds-1,+$('time').value+Math.floor(elapsed));elapsed%=1;draw();if(+$('time').value>=data.seconds-1){running=false;$('play').textContent='Play'}}}last=now;requestAnimationFrame(animate)}
draw();requestAnimationFrame(animate);
</script>'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("report", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    report = json.loads(args.report.read_text(encoding="utf-8"))
    bots = []
    for sample in report["bot_samples"]:
        state, vitals = sample.get("state") or {}, sample.get("vitals") or {}
        inventory = sample.get("inventory") or {}
        slot = inventory.get("selected")
        weapon = inventory.get("slots", [])[slot] if slot is not None else None
        bots.append({"id": sample["id"], "second": sample["second"], "position": state.get("feet"),
                     "alive": vitals.get("alive"), "team": vitals.get("team"), "spawn": vitals.get("spawn_tick"),
                     "behaviour": sample["behaviour"], "goal": sample["goal"], "next": sample["next"],
                     "weapon": weapon, "images": sample.get("images"), "goof": sample.get("goof"),
                     "acted": sample["acted"], "driving": sample.get("driving"), "decisions": sample.get("decisions"),
                     "team_evidence": sample.get("team"), "objective": sample.get("objective_detail"),
                     "diagnostic": sample.get("objective_diagnostic")})
    data = {"world": report["world"], "seconds": report["seconds"], "bots": bots,
            "vehicles": [{"id": s["id"], "second": s["second"], "position": s["position"]} for s in report["samples"]],
            "rounds": report.get("rounds", [])}
    encoded = json.dumps(data, separators=(",", ":")).replace("<", "\\u003c")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(HTML.replace("DATA", encoded), encoding="utf-8")


if __name__ == "__main__":
    main()
