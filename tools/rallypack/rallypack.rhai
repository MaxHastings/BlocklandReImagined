// Native shared Rallypack playtest policy. Configurations come from authored data.
fn fresh(p) {
    reset_ammo(p);
    set_player(p, "gun", #{last:-1000, release:-1000, shot:0, climb:0.0, bound:0.0,
        reloading:false, empty:false, bank:"", reload_image:"", tail:-1, tail_sound:"",
        charge:0, cook:-1, shield:false, distant:[]});
}
fn state(p) {
    let s = get_player(p, "gun");
    if !("last" in s) { fresh(p); s = get_player(p, "gun"); }
    s
}
fn current(p) { let me=player(p); if me == () { () } else { config(me.image) } }
fn show(p, c) {
    let a=get_player(p, c.bank);
    bottom_print(p, `Rallypack: ${a.n}/${c.size}   Spares: ${a.mags}   Reserve: ${a.reserve}`, 10.0);
}
fn on_loadout(p) { fresh(p); }
fn on_death(p, killer) { fresh(p); set_speed_scale(p,1.0); set_fov(p,()); }
fn cmd_mount(p) {
    let c=current(p); if c==() { return; }
    let s=state(p);
    if s.bank != c.bank { s.shot=0; s.climb=0.0; s.bound=0.0; s.last=-1000; }
    s.bank=c.bank;
    s.shield=player(p).image.contains("shieldpistol");
    mount_image(p,if s.shield {"weapon_rallypack:image/native_shield"}
        else {"weapon_rallypack:image/r_shielddropimagenooffset"},2);
    if !c.reloading { s.reloading=false; }
    set_player(p,"gun",s);
    set_speed_scale(p,c.movement);
    set_fov(p,if c.aimed {if c.category=="sniper" {30.0} else {75.0}} else {()});
    set_image_ammo(p,get_player(p,c.bank).n>0);
    show(p,c);
}
fn stop(p,c) {
    let s=state(p);
    if s.shot>0 && s.tail<0 && c.tail!="" { s.tail=tick()+6; s.tail_sound=c.tail; }
    s.release=tick(); set_player(p,"gun",s);
}
fn cmd_unmount(p) {
    if current(p)!=() { return; }
    let s=state(p); s.reloading=false; s.cook=-1;
    if s.shield {mount_image(p,"weapon_rallypack:image/r_shielddropimagenooffset",2);s.shield=false;}
    set_player(p,"gun",s); set_speed_scale(p,1.0);set_fov(p,());
}
fn cmd_ready(p) { let c=current(p); if c!=() {stop(p,c);} }
fn cmd_check(p) {
    let c=current(p);if c==() {return;}
    let a=get_player(p,c.bank);set_image_ammo(p,a.n>0);
    if a.n<=0 && !c.reloading && c.reload!="" {cmd_reload(p);}
}
fn cmd_reservecheck(p) {
    let c=current(p);if c==() {return;}
    set_image_ammo(p,get_player(p,c.bank).reserve>0);
}
fn cmd_reload(p) {
    let c=current(p);if c==() || c.reload=="" {return;}
    let s=state(p);if s.reloading {return;}
    let a=get_player(p,c.bank);let largest=0;
    for n in a.mags {if n>largest {largest=n;}}
    if c.kind=="magazine" && (largest<=0 || largest<a.n) {return;}
    if c.kind!="magazine" && (a.reserve<=0 || a.n>=c.size) {return;}
    stop(p,c);s=state(p);s.reloading=true;s.empty=a.n==0;s.reload_image=c.reload;
    set_player(p,"gun",s);mount_image(p,c.reload);set_image_ammo(p,a.n>0);
    set_speed_scale(p,1.0);set_fov(p,());
}
fn cmd_finish(p) {
    let c=current(p);if c==() {return;}
    let s=state(p);let a=get_player(p,c.bank);
    if c.kind=="magazine" && s.reloading {
        let best=-1;let largest=0;
        for i in 0..a.mags.len() {if a.mags[i]>largest {largest=a.mags[i];best=i;}}
        if best>=0 {
            a.mags.remove(best);let old=a.n;
            if c.plus && old>0 {largest+=1;old-=1;}
            if old>0 {a.mags.push(old);}
            a.n=largest;set_player(p,c.bank,a);
        }
    }
    s.reloading=false;set_player(p,"gun",s);mount_image(p,c.base);
    set_image_ammo(p,a.n>0);show(p,c);
}
fn cmd_loadone(p) {
    let c=current(p);if c==() {return;}
    let a=get_player(p,c.bank);let s=state(p);
    let max=c.size;if c.category=="shotgun" && !s.empty {max+=1;}
    if a.n<max && a.reserve>0 {a.n+=1;a.reserve-=1;set_player(p,c.bank,a);}
    let me=player(p);play_thread(p,2,"plant");
    sound_at("weapon_rallypack:sound/r_shotgunloadsound",me.x,me.y,me.z);show(p,c);
}
fn cmd_fullcheck(p) {
    let c=current(p);if c==() {return;}
    let a=get_player(p,c.bank);let s=state(p);let max=c.size;
    if c.category=="shotgun" && !s.empty {max+=1;}
    set_image_ammo(p,a.n>=max || a.reserve<=0);
}
fn cmd_boltcheck(p) {
    let c=current(p);if c==() {return;}
    let a=get_player(p,c.bank);set_image_ammo(p,a.n>=c.size-1 || a.reserve<=1);
}
fn cmd_chambercheck(p) {set_image_ammo(p,state(p).empty);}
fn cmd_inline_reload(p) { cmd_loadone(p); }
fn cmd_empty(p) {
    let c=current(p);if c==() {return;}
    let a=get_player(p,c.bank);a.reserve+=a.n;a.n=0;set_player(p,c.bank,a);
}
fn cmd_stripper(p) {
    let c=current(p);if c==() {return;}
    let a=get_player(p,c.bank);
    while a.n<c.size && a.reserve>0 {a.n+=1;a.reserve-=1;}
    set_player(p,c.bank,a);show(p,c);
}
fn cmd_dry(p) {
    let c=current(p);if c==() {return;}
    let me=player(p);sound_at("weapon_rallypack:sound/r_submachinereload3sound",me.x,me.y,me.z);
    show(p,c);
}
fn cmd_aim(p) {
    let c=current(p);if c==() || c.ads=="" || state(p).reloading {return;}
    mount_image(p,c.ads);
}
fn cmd_charge(p) {let s=state(p);s.charge=tick();set_player(p,"gun",s);}
fn cmd_prime(p) {mount_image(p,"weapon_rallypack:image/r_m67primedimage");cmd_charge(p);}
fn cmd_cook(p) {let s=state(p);s.cook=tick();set_player(p,"gun",s);}
fn cmd_detonate(p) {
    spawn_explosion(p,"weapon_rallypack:projectile/r_m67projectile",1.0);
    unmount_image(p);
}
fn cmd_supply(p) {reset_ammo(p);let c=current(p);if c!=() {show(p,c);}}
fn noise(seed) {let v=(seed.to_float()*12.9898).sin()*43758.5453;v-v.floor()}
fn cmd_shot(p) {
    let c=current(p);let me=player(p);if c==() || c.projectile=="" || me==() {return;}
    let s=state(p);let a=get_player(p,c.bank);
    if (a.n<=0 && c.category!="bow") || s.reloading {return;}
    if tick()-s.last<31 || tick()-s.release<24 {s.shot+=1;}
    else {s.shot=1;s.climb=0.0;s.bound=0.0;}
    let dx=me.lx+me.lz*s.bound;let dy=me.ly+s.climb;let dz=me.lz-me.lx*s.bound;
    let cm=c.cm;let bm=c.bm;if me.crouched {bm*=0.25;}
    if s.shot>3 && c.category=="gun" {
        let digit=c.seed[s.shot%c.seed.len()];
        let h=[5.0,-4.0,7.0,9.0,0.0,1.0,-6.0,2.0,8.0,-2.0];
        let v=[3.0,-2.0,6.0,-1.0,4.0,6.0,-5.0,9.0,-3.0,5.0];
        let m=(cm+c.bm)*0.5*3.5*0.002*(1.0-s.shot.to_float()/c.size.to_float());
        dx+=me.lz*h[digit]*m;dz-=me.lx*h[digit]*m;dy+=v[digit]*m;
    }
    s.climb=if s.shot>=3 && c.category=="gun" {(s.shot-2).to_float().ln()*cm*0.1} else {0.0};
    s.bound=if s.shot>=3 && c.category=="gun" {bm/(s.shot-1).to_float()*(0.3141592653589793*(s.shot-3).to_float()).sin()} else {0.0};
    let speed=c.speed;let projectile=c.projectile;
    if c.category=="bow" {
        speed=(tick()-s.charge).to_float()/120.0*100.0;
        if speed<50.0 {speed=50.0;}if speed>200.0 {speed=200.0;}
        if speed>=180.0 {projectile="weapon_rallypack:projectile/r_arrowprojectile3";}
        else if speed>100.0 {projectile="weapon_rallypack:projectile/r_arrowprojectile2";}
    }
    for i in 0..c.pellets {
        let x=dx;let y=dy;let z=dz;
        if c.spread>0.0 {
            let seed=tick()*31+p*13+i*7;
            x+=(noise(seed)-0.5)*c.spread;y+=(noise(seed+1)-0.5)*c.spread;z+=(noise(seed+2)-0.5)*c.spread;
        }
        fire(projectile,me.mx,me.my,me.mz,x*speed,y*speed,z*speed,p);
    }
    if c.category!="bow" {a.n-=1;set_player(p,c.bank,a);set_image_ammo(p,a.n>0);}
    if c.category=="grenade" {unmount_image(p);}
    let shake=0.05*s.shot.to_float();if shake<0.1 {shake=0.1;}if shake>0.55 {shake=0.55;}
    spawn_explosion(p,"weapon_rallypack:projectile/r_shotgunrecoilprojectile",shake);
    if s.shot>3 {play_thread(p,2+s.shot%2,"jump");}
    s.last=tick();s.tail=-1;s.tail_sound=c.tail;
    if c.far!="" {
        for person in players() {
            let t=person;let x=t.x-me.x;let y=t.y-me.y;let z=t.z-me.z;let distance=(x*x+y*y+z*z).sqrt();
            if distance>=95.0 && s.distant.len()<32 {s.distant.push(#{to:t.id,due:tick()+(distance*0.6).ceil().to_int(),sound:c.far});}
        }
    }
    set_player(p,"gun",s);show(p,c);
}
fn on_tick() {
    for person in players() {
        let p=person.id;let s=get_player(p,"gun");if !("last" in s) {continue;}
        let me=person;
        if s.tail>=0 && tick()>=s.tail {
            if s.tail_sound!="" {sound_at(s.tail_sound,me.x,me.y,me.z);}
            s.tail=-1;
        }
        let waiting=[];
        for cue in s.distant {if tick()>=cue.due {play_sound(cue.to,cue.sound);} else {waiting.push(cue);}}
        s.distant=waiting;
        if s.reloading && me.image!=s.reload_image {s.reloading=false;}
        set_player(p,"gun",s);
    }
}
fn on_drop(p,item,slot) {
    let c=item_config(item);if c==() {()} else {#{rounds:get_player(p,c.bank).n}}
}
fn on_pickup(p,item,info) {
    if item.starts_with("weapon_rallypack:weapon/r_ammo") {reset_ammo(p);return "take";}
    let c=item_config(item);
    if c!=() && info.data!=() && "rounds" in info.data {
        state(p);let a=get_player(p,c.bank);a.n=info.data.rounds;set_player(p,c.bank,a);
    }
    ()
}
fn on_damage(victim,attacker,amount,info) {
    let me=player(victim);
    if me==() || !me.image.contains("shieldpistol") || !info.direct || !("dx" in info) {return ();}
    let front=info.dx*me.lx+info.dy*me.ly+info.dz*me.lz;
    if front< -0.4 && (info.region!="legs" || me.crouched) {0.0} else {()}
}
