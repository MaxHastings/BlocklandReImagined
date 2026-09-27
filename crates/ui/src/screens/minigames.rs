//! Stock v20 mini-game list, rule editor and invitation dialog.
//! Views use the original pack layouts; controls are typed and host-gated.
use super::*;
use crate::{api::*, geom::Rect, ui::Callback, view::{EventKind, Value}};

#[derive(Clone, Copy)]
enum Kind { List, Rules, Invite }

pub struct MiniGameScreen { id: ScreenId, kind: Kind, view: View, selected_game: Option<MiniGameId>, game_ids: Vec<MiniGameId>, draft: MiniGameRules, types: Vec<MiniGameChoice>, items: Vec<MiniGameChoice>, loaded_revision: Option<(bool,u64)>, request: Option<RequestId> }
impl MiniGameScreen {
    pub fn list(core: &Core) -> Self { Self::new(core, Kind::List) }
    pub fn settings(core: &Core) -> Self { Self::new(core, Kind::Rules) }
    pub fn invitation(core: &Core) -> Self { Self::new(core, Kind::Invite) }
    fn new(core: &Core, kind: Kind) -> Self {
        let (id, layout) = match kind {
            Kind::List => (ScreenId::MiniGames, "joinMiniGameGui"),
            Kind::Rules => (ScreenId::MiniGameSettings, "CreateMiniGameGui"),
            Kind::Invite => (ScreenId::MiniGameInvitation, "MiniGameInviteGui"),
        };
        let mut s = Self { id, kind, view: layout_view(core, layout), selected_game: None, game_ids: vec![],
            draft: core.minigames.rules_draft(), types: vec![], items: vec![], loaded_revision: None, request: None };
        s.refresh(core);
        s
    }
    fn selected_game(&self) -> Option<MiniGameId> { self.view.id("JMG_List").and_then(|n|self.view.selected(n))
        .and_then(|i|usize::try_from(i).ok()).and_then(|i|self.game_ids.get(i)).copied() }
    fn set_active(&mut self, command: &str, active: bool) {
        if let Some(n) = self.view.by_command(command) { self.view.set_active(n, active); }
    }
    fn set_var(&mut self, key: &str, value: &str) {
        let node = { self.view.walk().find(|&n| self.view.node(n).ctrl.variable.as_deref().is_some_and(|v| v.eq_ignore_ascii_case(key))) };
        if let Some(n) = node {
            if matches!(self.view.node(n).state.value, Value::Bool(_)) { self.view.set_bool(n, value == "1" || value.eq_ignore_ascii_case("true")); }
            else { self.view.set_text(n, value); }
        }
    }
    fn variable(&self, key: &str) -> Option<usize> {
        self.view.walk().find(|&n| self.view.node(n).ctrl.variable.as_deref().is_some_and(|v| v.eq_ignore_ascii_case(key)))
    }
    fn write_rules(&mut self, rules: &MiniGameRules) {
        for (key,value) in [
            ("$MiniGame::Title",rules.title.clone()), ("$MiniGame::Points::BreakBrick",rules.points_break_brick.to_string()),
            ("$MiniGame::Points::PlantBrick",rules.points_plant_brick.to_string()), ("$MiniGame::Points::KillPlayer",rules.points_kill_player.to_string()),
            ("$MiniGame::Points::KillSelf",rules.points_kill_self.to_string()), ("$MiniGame::Points::Die",rules.points_die.to_string()),
            ("$MiniGame::RespawnTime",rules.respawn_seconds.to_string()), ("$MiniGame::VehicleRespawnTime",rules.vehicle_respawn_seconds.to_string()),
            ("$MiniGame::BrickRespawnTime",rules.brick_respawn_seconds.to_string()),
        ] { self.set_var(key, &value); }
        for (key,value) in [
            ("$MiniGame::InviteOnly",rules.invite_only),("$MiniGame::UseAllPlayersBricks",rules.use_all_players_bricks),
            ("$MiniGame::PlayersUseOwnBricks",rules.players_use_own_bricks),("$MiniGame::UseSpawnBricks",rules.use_spawn_bricks),
            ("$MiniGame::FallingDamage",rules.falling_damage),("$MiniGame::WeaponDamage",rules.weapon_damage),
            ("$MiniGame::SelfDamage",rules.self_damage),("$MiniGame::VehicleDamage",rules.vehicle_damage),
            ("$MiniGame::BrickDamage",rules.brick_damage),("$MiniGame::EnableWand",rules.enable_wand),
            ("$MiniGame::EnableBuilding",rules.enable_building),("$MiniGame::EnablePainting",rules.enable_painting),
        ] { self.set_var(key, if value {"1"} else {"0"}); }
    }
    fn fill_choice(&mut self, node: &str, choices: &[MiniGameChoice], selected: Option<&str>) {
        if let Some(n) = self.view.id(node) {
            let mut items = vec![(" NONE".to_string(), 0)];
            items.extend(choices.iter().enumerate().map(|(i,c)| (c.name.clone(), (i+1) as i64)));
            self.view.nodes[n].state.items = items;
            let index = choices.iter().position(|c| Some(c.id.as_str()) == selected).map_or(0,|i|(i+1) as i64);
            self.view.select(n, Some(index));
        }
    }
    fn apply_rules_state(&mut self, core: &Core) {
        self.draft = core.minigames.rules_draft();
        self.write_rules(&self.draft.clone());
        if let Some(n) = self.view.id("CMG_PlayerDataBlock") {
            self.view.nodes[n].state.items = core.minigames.player_types.iter().enumerate()
                .map(|(i,c)| (c.name.clone(),i as i64)).collect();
            self.view.select(n, core.minigames.player_types.iter().position(|c| c.id == self.draft.player_type).map(|i| i as i64));
        }
        for i in 0..5 {
            let selected=self.draft.loadout[i].clone();
            self.fill_choice(&format!("CMG_StartEquip{i}"), &core.minigames.items, selected.as_deref());
        }
        if let Some(n) = self.view.id("CMG_ColorList") {
            self.view.nodes[n].state.items = core.minigames.colors.iter().map(|c| (c.name.clone(), i64::from(c.index))).collect();
            let selected=core.minigames.games.iter().find(|g| Some(g.id)==core.minigames.active_game).map(|g|g.color);
            self.view.select(n, selected.map(i64::from).or_else(||core.minigames.colors.first().map(|c|i64::from(c.index))));
        }
        if let (Some(n),Some(color)) = (self.view.id("CMG_Swatch"), core.minigames.colors.iter().find(|c| Some(i64::from(c.index)) == self.view.id("CMG_ColorList").and_then(|v|self.view.selected(v)))) {
            self.view.nodes[n].state.tint=Some([color.rgb[0],color.rgb[1],color.rgb[2],255]);
        }
        let owns=core.minigames.owns_active_game;
        let mode_edit=core.minigames.active_game.is_some()&&owns;
        if let Some(n)=self.view.id("CMG_Window"){self.view.set_text(n,if mode_edit{"Edit Mini-Game"}else{"Create Mini-Game"});}
        if let Some(n)=self.view.id("CMG_CreateButton"){self.view.set_text(n,if mode_edit{"Update >>"}else{"Create >>"});}
        if let Some(n)=self.view.id("CMG_ColorBlocker"){self.view.set_visible(n,mode_edit);}
        if let Some(n)=self.view.id("CMG_EndBlocker"){self.view.set_visible(n,!owns);}
        let can_save=if mode_edit {core.minigames.can(crate::models::minigames::Operation::Configure)} else {core.minigames.can(crate::models::minigames::Operation::Create)};
        self.set_active("CreateMiniGameGui.clickCreate();",can_save&&self.request.is_none());
        self.set_active("CreateMiniGameGui.clickReset();",core.minigames.can(crate::models::minigames::Operation::Reset)&&self.request.is_none());
        self.set_active("CreateMiniGameGui.clickEnd();",core.minigames.can(crate::models::minigames::Operation::End)&&self.request.is_none());
    }
    fn read_rules(&self) -> Result<MiniGameRules,String> {
        let val=|key:&str|self.variable(key).map(|n|self.view.edit_text(n)).unwrap_or_default();
        let boolean=|key:&str|self.variable(key).is_some_and(|n|self.view.bool_value(n));
        let number=|key:&str,min:u32,max:u32|->Result<u32,String>{
            let value=val(key).trim().parse::<u32>().map_err(|_|format!("Enter a number for {key}."))?;
            if !(min..=max).contains(&value){return Err(format!("{key} must be {min}–{max}."));} Ok(value)
        };
        let mut rules=self.draft.clone();
        rules.title=val("$MiniGame::Title");
        if rules.title.trim().is_empty()||rules.title.chars().count()>35||rules.title.chars().any(char::is_control){return Err("Title must contain 1–35 visible characters.".into());}
        rules.points_break_brick=val("$MiniGame::Points::BreakBrick").parse().map_err(|_|"Break-brick points must be an integer.")?;
        rules.points_plant_brick=val("$MiniGame::Points::PlantBrick").parse().map_err(|_|"Plant-brick points must be an integer.")?;
        rules.points_kill_player=val("$MiniGame::Points::KillPlayer").parse().map_err(|_|"Kill-player points must be an integer.")?;
        rules.points_kill_self=val("$MiniGame::Points::KillSelf").parse().map_err(|_|"Kill-self points must be an integer.")?;
        rules.points_die=val("$MiniGame::Points::Die").parse().map_err(|_|"Death points must be an integer.")?;
        rules.respawn_seconds=number("$MiniGame::RespawnTime",1,30)?;
        rules.vehicle_respawn_seconds=number("$MiniGame::VehicleRespawnTime",0,300)?;
        rules.brick_respawn_seconds=number("$MiniGame::BrickRespawnTime",2,300)?;
        rules.invite_only=boolean("$MiniGame::InviteOnly"); rules.use_all_players_bricks=boolean("$MiniGame::UseAllPlayersBricks");
        rules.players_use_own_bricks=boolean("$MiniGame::PlayersUseOwnBricks"); rules.use_spawn_bricks=boolean("$MiniGame::UseSpawnBricks");
        rules.falling_damage=boolean("$MiniGame::FallingDamage"); rules.weapon_damage=boolean("$MiniGame::WeaponDamage");
        rules.self_damage=boolean("$MiniGame::SelfDamage"); rules.vehicle_damage=boolean("$MiniGame::VehicleDamage");
        rules.brick_damage=boolean("$MiniGame::BrickDamage"); rules.enable_wand=boolean("$MiniGame::EnableWand");
        rules.enable_building=boolean("$MiniGame::EnableBuilding"); rules.enable_painting=boolean("$MiniGame::EnablePainting");
        if let Some(n)=self.view.id("CMG_PlayerDataBlock").and_then(|n|self.view.selected(n)).and_then(|i|usize::try_from(i).ok()).and_then(|i|self.draft_type_id(i)){rules.player_type=n;}
        for i in 0..5 { let selected=self.view.id(&format!("CMG_StartEquip{i}")).and_then(|n|self.view.selected(n)).unwrap_or(0);
            rules.loadout[i]=usize::try_from(selected).ok().and_then(|i|self.draft_item_id(i)); }
        Ok(rules)
    }
    fn draft_type_id(&self,index:usize)->Option<String>{ self.types.get(index).map(|c|c.id.clone()) }
    fn draft_item_id(&self,index:usize)->Option<String>{ if index==0 {None} else {self.items.get(index-1).map(|c|c.id.clone())} }
    fn refresh(&mut self,core:&Core){
        match self.kind {
            Kind::List=>{
                let old=self.selected_game;
                let games=&core.minigames.games;
                self.selected_game=core.minigames.retain_game_target(old).or_else(||games.first().map(|g|g.id));
                if let Some(n)=self.view.id("JMG_List"){
                    self.game_ids=games.iter().map(|g|g.id).collect();
                    self.view.nodes[n].state.items=games.iter().enumerate().map(|(i,g)|(format!("{}\t{}\t{}\t{}",g.title,g.owner_name,g.member_count,if g.invite_only{"Invite only"}else{"Public"}),i as i64)).collect();
                    self.view.select(n,self.selected_game.and_then(|id|self.game_ids.iter().position(|g|*g==id)).map(|i|i as i64));
                }
                let selected=games.iter().find(|g|Some(g.id)==self.selected_game);
                let join=selected.is_some_and(|g|!g.invite_only)&&core.minigames.can(crate::models::minigames::Operation::Join)&&core.minigames.active_game.is_none()&&self.request.is_none();
                self.set_active("JoinMiniGameGui.clickJoin();",join);
                self.set_active("JoinMiniGameGui.clickLeave();",core.minigames.active_game.is_some()&&self.request.is_none());
                self.set_active("JoinMiniGameGui.clickCreate();",(core.minigames.can(crate::models::minigames::Operation::Create)||core.minigames.can(crate::models::minigames::Operation::Configure))&&self.request.is_none());
                for (name,shown) in [("JMG_JoinBlocker",!join),("JMG_LeaveBlocker",core.minigames.active_game.is_none()),("JMG_CreateBlocker",!core.minigames.can(crate::models::minigames::Operation::Create)&&!core.minigames.can(crate::models::minigames::Operation::Configure))]{if let Some(n)=self.view.id(name){self.view.set_visible(n,shown);}}
                self.status(core);
            }
            Kind::Rules=>{
                if self.loaded_revision != Some((core.minigames.ready,core.minigames.revision)) {
                    self.types=core.minigames.player_types.clone(); self.items=core.minigames.items.clone();
                    self.apply_rules_state(core); self.loaded_revision=Some((core.minigames.ready,core.minigames.revision));
                }
                self.status(core);
            }
            Kind::Invite=>{
                if core.minigames.invitations.is_empty() { return; }
                if let Some(i)=core.minigames.invitations.last(){for (name,value) in [("MGI_Title",i.title.as_str()),("MGI_Name",i.owner_name.as_str()),("MGI_BL_ID",i.owner_display_id.as_str())]{if let Some(n)=self.view.id(name){self.view.set_text(n,value);}}}
                for cmd in ["MiniGameInviteGui.clickAccept();","MiniGameInviteGui.clickReject();","MiniGameInviteGui.clickIgnore();"]{self.set_active(cmd,core.minigames.can(crate::models::minigames::Operation::AcceptInvite)&&self.request.is_none());}
            }
        }
    }
    fn status(&mut self,core:&Core){
        let key=match self.kind {Kind::List=>"NativeMiniGameListStatus",Kind::Rules=>"NativeMiniGameRulesStatus",Kind::Invite=>"NativeMiniGameInviteStatus"};
        let status=if !core.minigames.ready{"Mini-game controls are unavailable until the host provides session state.".to_string()}else{core.minigames.status.clone()};
        if let Some(n)=self.view.id(key){self.view.set_text(n,&status);}else{
            let parent=window(&self.view).unwrap_or(self.view.root);
            let mut c=text("GuiTextProfile",Rect::new(12,440,560,24),&status); c.name=Some(key.into());c.class="GuiMLTextCtrl".into();self.view.add(parent,c);
        }
    }
}

impl Screen for MiniGameScreen {
    fn id(&self)->ScreenId{self.id}
    fn view(&self)->&View{&self.view}
    fn view_mut(&mut self)->&mut View{&mut self.view}
    fn blocks_accelerators(&self)->bool{true}
    fn on_wake(&mut self,core:&mut Core){
        if matches!(self.kind,Kind::List)&&core.minigames.can(crate::models::minigames::Operation::List){
            self.request=core.minigame_request(MiniGameOperation::List,UiAction::RequestMiniGameList);
        }
        self.refresh(core);
    }
    fn on_sleep(&mut self,core:&mut Core){if let Some(id)=self.request.take(){core.pending.remove(&id);}}
    fn on_update(&mut self,core:&mut Core){
        if matches!(self.kind,Kind::Invite)&&core.minigames.invitations.is_empty(){core.pop(self.id);return;}
        self.refresh(core);
    }
    fn on_result(&mut self,id:RequestId,kind:Option<&Pending>,result:&Result<(),String>,core:&mut Core)->bool{
        if self.request!=Some(id)||!matches!(kind,Some(Pending::MiniGame(_))){return false;}
        self.request=None;
        core.minigames.status=result.as_ref().map_or_else(|e|e.clone(),|_|"Mini-game request completed.".into());
        self.refresh(core); true
    }
    fn on_key(&mut self,key:Key,_:Modifiers,core:&mut Core)->bool{
        if key==Key::Escape{core.pop(self.id);true}else{false}
    }
    fn on_event(&mut self,ev:&ViewEvent,core:&mut Core){
        if !self.view.node(ev.node).state.active{return;}
        if ev.kind==EventKind::Close{core.pop(self.id);return;}
        if ev.kind==EventKind::Changed {
            if self.view.node(ev.node).ctrl.name.as_deref()==Some("JMG_List"){
                self.selected_game=self.selected_game(); self.refresh(core);
            } else if self.view.node(ev.node).ctrl.name.as_deref()==Some("CMG_ColorList"){
                let selected=self.view.id("CMG_ColorList").and_then(|n|self.view.selected(n));
                if let (Some(n),Some(color))=(self.view.id("CMG_Swatch"),core.minigames.colors.iter().find(|c|Some(i64::from(c.index))==selected)){
                    self.view.nodes[n].state.tint=Some([color.rgb[0],color.rgb[1],color.rgb[2],255]);
                }
            }
            return;
        }
        if !matches!(ev.kind,EventKind::Click|EventKind::Submit|EventKind::DoubleClick){return;}
        let cmd=command_of(&self.view,ev.node).to_ascii_lowercase();
        match self.kind {
            Kind::List=>match cmd.as_str(){
                "canvas.popdialog(joinminigamegui);"=>core.pop(self.id),
                "joinminigamegui.clicklist();"=>{self.selected_game=self.selected_game();self.refresh(core);},
                "joinminigamegui.clickjoin();"=>if let Some(game)=self.selected_game(){
                    self.request=core.minigame_request(MiniGameOperation::Join,UiAction::JoinMiniGame{game});self.refresh(core);
                },
                "joinminigamegui.clickleave();"=>if let Some(game)=core.minigames.active_game{
                    if core.minigames.owns_active_game { core.message_yes_no("End Mini-Game?","Are you sure you want to end the mini-game?",Callback::MiniGame{game,operation:MiniGameOperation::End}); }
                    else if let Some(id)=core.minigame_request(MiniGameOperation::Leave,UiAction::LeaveMiniGame{game}){self.request=Some(id);self.refresh(core);}
                },
                "joinminigamegui.clickcreate();"=>{core.pop(self.id);core.push(ScreenId::MiniGameSettings);},
                _=>{
                    let col=cmd.strip_prefix("joinminigamegui.sortlist(").or_else(||cmd.strip_prefix("joinminigamegui.sortnumlist("))
                        .and_then(|s|s.strip_suffix(");")).and_then(|s|s.parse::<usize>().ok());
                    if col.is_some(){self.refresh(core);}
                }
            },
            Kind::Rules=>match cmd.as_str(){
                "canvas.popdialog(createminigamegui);"=>core.pop(self.id),
                "createminigamegui.clickcreate();"=>match self.read_rules(){
                    Err(e)=>core.minigames.status=e,
                    Ok(rules)=>{
                        let editing=core.minigames.owns_active_game;
                        let color=self.view.id("CMG_ColorList").and_then(|n|self.view.selected(n)).and_then(|v|u8::try_from(v).ok()).unwrap_or(0);
                        let request=if editing {core.minigames.active_game.and_then(|game|core.minigame_request(MiniGameOperation::Configure,UiAction::ConfigureMiniGame{game,rules}))}
                            else{core.minigame_request(MiniGameOperation::Create,UiAction::CreateMiniGame{color,rules})};
                        self.request=request;self.refresh(core);
                    }
                },
                "createminigamegui.clickreset();"=>if let Some(game)=core.minigames.active_game{
                    core.message_yes_no("Reset Mini-Game?","Reset the mini-game and restore its spawn bricks?",Callback::MiniGame{game,operation:MiniGameOperation::Reset});
                },
                "createminigamegui.clickend();"=>if let Some(game)=core.minigames.active_game{
                    core.message_yes_no("End Mini-Game?","Are you sure you want to end the mini-game?",Callback::MiniGame{game,operation:MiniGameOperation::End});
                },
                "createminigamegui.clickcolorlist();"=>self.refresh(core),
                _=>{}
            },
            Kind::Invite=>{
                let Some(invite)=core.minigames.invitations.last().cloned() else{return;};
                match cmd.as_str(){
                    "minigameinvitegui.clickaccept();"=>{self.request=core.minigame_request(MiniGameOperation::AcceptInvite,UiAction::AcceptMiniGameInvite{game:invite.game});},
                    "minigameinvitegui.clickreject();"=>{self.request=core.minigame_request(MiniGameOperation::RejectInvite,UiAction::RejectMiniGameInvite{game:invite.game,ignore_owner:false});},
                    "minigameinvitegui.clickignore();"=>core.message_yes_no("Ignore User?","Are you sure you want to ignore mini-game invites from this user?",Callback::MiniGame{game:invite.game,operation:MiniGameOperation::IgnoreInvite}),
                    _=>{}
                }
            }
        }
        self.refresh(core);
    }
}
