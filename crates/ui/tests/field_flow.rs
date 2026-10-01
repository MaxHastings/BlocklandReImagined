//! Every value a player can enter on a screen that talks to the game reaches
//! what that screen sends, and lands in the right field.
//!
//! Drives the real converted v20 screens (`content/ui-pack-004`) only through
//! input a player makes: mouse clicks at a control's centre, typed
//! characters and keys. For each screen, every visible editable control is
//! changed on its own: a unique value is typed into each text box, each
//! checkbox is clicked, each other radio button is picked and each dropdown
//! moves to another entry. The screen's submit button is then clicked, and
//! what the screen emitted (its `UiAction`s and the saved settings) is
//! compared, as JSON, with a run that changed nothing. The paths that
//! differ must be exactly the ones this file expects for that control, and
//! a typed value must be the value found there.
//!
//! This catches a screen that reads the wrong widget (a label instead of the
//! typed text, one box instead of another), a control nothing reads, and a
//! control the mouse cannot reach. Controls deliberately not sent are listed
//! with the reason. A control on screen that no table names fails the test,
//! so new controls must be accounted for.
//!
//! Content-backed: prints a loud "skipped" and passes when ui-pack-004 is not
//! converted on this machine (GitHub CI).
//! Run: cargo test -p bri-ui --test field_flow -- --nocapture
//! `BRI_FIELD_FLOW_PRINT=1` prints the observed table for every control.
use bri_ui::{
    api::*,
    binds::Platform,
    input::{InputEvent, Key, Modifiers, MouseButton},
    models::admin::{
        AdminFeature, AdminOptions, AdminPlayer, AdminRole, AdminSnapshot, AdminUpdate,
    },
    pack::Pack,
    screens::ScreenId,
    ui::{Ui, UiConfig},
    view::View,
};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    rc::Rc,
};

const EDITABLE: [&str; 5] = [
    "GuiTextEditCtrl",
    "GuiMLTextEditCtrl",
    "GuiCheckBoxCtrl",
    "GuiRadioCtrl",
    "GuiPopUpMenuCtrl",
];

fn pack() -> Option<Rc<Pack>> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/ui-pack-004");
    if !dir.join("ui-pack.json").exists() {
        eprintln!(
            "\n**** skipped: field_flow needs content/ui-pack-004, which is not converted on this machine ****\n"
        );
        return None;
    }
    Some(Rc::new(Pack::load(&dir).expect("ui-pack-004 loads")))
}

fn new_ui(pack: &Rc<Pack>) -> Ui {
    let mut u = Ui::new(
        pack.clone(),
        UiConfig {
            size: (1024, 768),
            scale: Some(1.0),
            platform: Platform::Windows,
        },
        Settings {
            binds: Some(vec![]),
            ..Default::default()
        },
    );
    u.update(0);
    u
}

// ------------------------------------------------------------ player input

fn press(u: &mut Ui, key: Key, mods: Modifiers) {
    u.handle_input(InputEvent::KeyDown {
        key,
        mods,
        repeat: false,
    });
    u.handle_input(InputEvent::KeyUp { key, mods });
    u.update(0);
}

fn click_at(u: &mut Ui, (x, y): (f32, f32)) {
    u.handle_input(InputEvent::MouseMove { x, y });
    u.handle_input(InputEvent::MouseDown {
        button: MouseButton::Left,
        x,
        y,
    });
    u.handle_input(InputEvent::MouseUp {
        button: MouseButton::Left,
        x,
        y,
    });
    u.update(0);
}

fn view(u: &Ui, screen: ScreenId) -> &View {
    u.screen(screen)
        .unwrap_or_else(|| panic!("{screen:?} is not open: {:?}", u.stack()))
        .view()
}

/// Scroll the scroll control holding `node` with the mouse wheel until the
/// control is inside its visible area, as a player would.
fn reveal(u: &mut Ui, screen: ScreenId, node: usize) {
    for _ in 0..200 {
        let v = view(u, screen);
        let mut scroll = v.node(node).parent;
        while let Some(p) = scroll {
            if v.node(p).ctrl.class == "GuiScrollCtrl" {
                break;
            }
            scroll = v.node(p).parent;
        }
        let Some(scroll) = scroll else { return };
        let (outer, r) = (v.node(scroll).rect, v.node(node).rect);
        let delta = if r.y < outer.y {
            1.0
        } else if r.bottom() > outer.bottom() {
            -1.0
        } else {
            return;
        };
        let s = u.scale();
        let (x, y) = ((outer.x + 4) as f32 * s, (outer.y + outer.h / 2) as f32 * s);
        u.handle_input(InputEvent::MouseMove { x, y });
        let before = view(u, screen).node(node).rect;
        u.handle_input(InputEvent::Wheel { delta });
        u.update(0);
        if view(u, screen).node(node).rect == before {
            return;
        }
    }
}

/// Where a player clicks `node`, in window pixels, once scrolled into view,
/// after checking a click there reaches it and not a control drawn over
/// it: the middle, or a checkbox's or radio button's box at its left (some
/// v20 labels run past their window).
fn centre(u: &mut Ui, screen: ScreenId, node: usize) -> Result<(f32, f32), String> {
    reveal(u, screen, node);
    let v = view(u, screen);
    let r = v.node(node).rect;
    let boxed = matches!(v.node(node).ctrl.class.as_str(), "GuiCheckBoxCtrl" | "GuiRadioCtrl");
    let x = if boxed { r.x + (r.h / 2).min(r.w / 2) } else { r.x + r.w / 2 };
    let y = r.y + r.h / 2;
    let mut hit = v.hit(x, y);
    while let Some(h) = hit {
        if h == node {
            let s = u.scale();
            return Ok((x as f32 * s, y as f32 * s));
        }
        hit = v.node(h).parent;
    }
    Err(format!(
        "a click at its centre reaches {:?}, not it",
        v.hit(x, y).map(|h| key_of(v, h))
    ))
}

/// Click a control by name, command or visible text.
fn try_click(u: &mut Ui, screen: ScreenId, control: &str) -> Result<(), String> {
    let v = u
        .screen(screen)
        .ok_or_else(|| format!("{screen:?} is not open: {:?}", u.stack()))?
        .view();
    let node = v
        .id(control)
        .or_else(|| v.by_command(control))
        .or_else(|| {
            v.walk()
                .find(|&n| v.is_shown(n) && v.text_of(n).trim() == control)
        })
        .ok_or_else(|| format!("{control} is not on {screen:?}"))?;
    let at = centre(u, screen, node).map_err(|e| format!("{control} on {screen:?}: {e}"))?;
    click_at(u, at);
    Ok(())
}

/// `try_click` for the steps that open a screen, which must work.
fn click(u: &mut Ui, screen: ScreenId, control: &str) {
    try_click(u, screen, control).unwrap_or_else(|e| panic!("{e}"));
}

/// Click the row of a list control whose text contains `shown`.
fn click_row(u: &mut Ui, screen: ScreenId, list: &str, shown: &str) -> Result<(), String> {
    let v = view(u, screen);
    let n = v.id(list).ok_or_else(|| format!("{list} is not on {screen:?}"))?;
    let items = &v.node(n).state.items;
    let Some((row, &(_, id))) = items
        .iter()
        .enumerate()
        .find(|(_, (text, _))| text.contains(shown))
    else {
        return Err(format!(
            "no row shows it; rows are {:?}",
            items.iter().map(|i| &i.0).collect::<Vec<_>>()
        ));
    };
    let r = v.node(n).rect;
    let h = v.node(n).state.row_height.max(1);
    let s = u.scale();
    click_at(u, ((r.x + 8) as f32 * s, (r.y + h * row as i32 + h / 2) as f32 * s));
    match view(u, screen).selected(n) {
        Some(sel) if sel == id => Ok(()),
        other => Err(format!("clicking it selects {other:?}, not row id {id}")),
    }
}

fn type_text(u: &mut Ui, text: &str) {
    for c in text.chars() {
        u.handle_input(InputEvent::Char(c));
    }
    u.update(0);
}

/// Click into a text box, clear it and type `text`.
fn replace_text(u: &mut Ui, screen: ScreenId, node: usize, text: &str) -> Result<(), String> {
    let at = centre(u, screen, node)?;
    click_at(u, at);
    if view(u, screen).focus != Some(node) {
        return Err("clicking it does not put the cursor in it".into());
    }
    press(u, Key::End, Modifiers::NONE);
    let len = view(u, screen).edit_text(node).chars().count();
    for _ in 0..len {
        press(u, Key::Backspace, Modifiers::NONE);
    }
    type_text(u, text);
    let now = view(u, screen).edit_text(node);
    if now != text {
        return Err(format!("typed {text:?} but the box holds {now:?}"));
    }
    Ok(())
}

// ------------------------------------------------------------ observation

/// A stable name for a control: its object name, else the preference it is
/// bound to, else its label.
fn key_of(v: &View, n: usize) -> String {
    let c = &v.node(n).ctrl;
    // Several v20 checkboxes share one object name; their variable tells
    // them apart.
    let shared_name = c.name.as_ref().is_some_and(|name| {
        v.walk()
            .filter(|&m| v.node(m).ctrl.name.as_ref() == Some(name))
            .count()
            > 1
    });
    match (&c.name, &c.variable) {
        (Some(name), _) if !shared_name => name.clone(),
        (_, Some(var)) => var.clone(),
        (Some(name), None) => format!("{name}:{}", v.text_of(n).trim()),
        (None, None) => format!("{}:{}", c.class, v.text_of(n).trim()),
    }
}

/// What a screen emitted: its actions (by variant; the settings mirror
/// `SaveSettings` is compared through `settings` instead) and the settings
/// it left behind.
fn snapshot(u: &mut Ui) -> Value {
    let mut actions = serde_json::Map::new();
    for (_, a) in u.drain_actions() {
        // Settings are compared through `settings`; previews and the typing
        // indicator say nothing about what was entered.
        if matches!(
            a,
            UiAction::SaveSettings(_)
                | UiAction::PreviewAvatar { .. }
                | UiAction::PreviewSave { .. }
                | UiAction::StartTyping
                | UiAction::StopTyping
        ) {
            continue;
        }
        let v = serde_json::to_value(&a).unwrap();
        let (variant, body) = match v {
            Value::Object(m) if m.len() == 1 => m.into_iter().next().unwrap(),
            Value::String(s) => (s, Value::Null),
            other => ("?".into(), other),
        };
        let mut name = variant.clone();
        let mut i = 1;
        while actions.contains_key(&name) {
            i += 1;
            name = format!("{variant}#{i}");
        }
        actions.insert(name, body);
    }
    let mut settings = serde_json::to_value(u.settings()).unwrap();
    // The first-run bind table is not something these screens edit.
    settings.as_object_mut().unwrap().remove("binds");
    serde_json::json!({ "actions": actions, "settings": settings })
}

fn leaves(v: &Value, path: String, out: &mut BTreeMap<String, Value>) {
    match v {
        Value::Object(m) => {
            for (k, v) in m {
                leaves(v, if path.is_empty() { k.clone() } else { format!("{path}.{k}") }, out);
            }
        }
        Value::Array(a) => {
            for (i, v) in a.iter().enumerate() {
                leaves(v, format!("{path}[{i}]"), out);
            }
        }
        _ => {
            out.insert(path, v.clone());
        }
    }
}

/// Paths whose value differs (or exists on one side only).
fn changed(a: &Value, b: &Value) -> BTreeMap<String, Value> {
    let (mut x, mut y) = (BTreeMap::new(), BTreeMap::new());
    leaves(a, String::new(), &mut x);
    leaves(b, String::new(), &mut y);
    let paths: BTreeSet<_> = x.keys().chain(y.keys()).cloned().collect();
    paths
        .into_iter()
        .filter(|p| x.get(p) != y.get(p))
        .map(|p| {
            let v = y.get(&p).cloned().unwrap_or(Value::Null);
            (p, v)
        })
        .collect()
}

// ------------------------------------------------------------- scenarios

/// What a control's value must do. Paths are JSON paths into what the
/// screen emitted: `actions.<Variant>.<field>` or `settings.<field>`.
#[derive(Clone, Copy)]
enum Expect {
    /// Exactly these paths change (a typed value must be what lands there).
    Sent(&'static [&'static str]),
    /// As `Sent`, typing this text instead of a made-up value (free-text
    /// boxes whose meaning depends on the words, such as a chat command).
    Typed(&'static str, &'static [&'static str]),
    /// Deliberately changes nothing the screen emits.
    NotSent(&'static str),
    /// Drawn but deliberately out of reach (greyed out behind a blocker).
    Blocked(&'static str),
}

/// A list whose chosen row picks what the screen acts on: choosing the row
/// showing the text must send the value at `path`.
struct Rows {
    list: &'static str,
    path: &'static str,
    rows: &'static [(&'static str, &'static str)],
}

struct Scenario {
    name: &'static str,
    /// Brings the screen up as the game would (updates from the host,
    /// clicks through the menus), leaving it on top.
    open: fn(&mut Ui),
    screen: ScreenId,
    /// Clicked to send; `None` presses Enter in the focused box.
    submit: Option<&'static str>,
    /// Buttons clicked after submit (confirmation dialogs), with their screen.
    confirm: &'static [(ScreenId, &'static str)],
    expect: &'static [(&'static str, Expect)],
    rows: Option<Rows>,
}

/// One probe: how the control was changed.
enum Change {
    Typed(String),
    Clicked,
    Picked,
}

fn open(pack: &Rc<Pack>, sc: &Scenario) -> Result<Ui, String> {
    let mut u = new_ui(pack);
    (sc.open)(&mut u);
    if u.top_id() != sc.screen {
        return Err(format!(
            "opening left {:?} on top, not {:?}",
            u.top_id(),
            sc.screen
        ));
    }
    u.drain_actions();
    Ok(u)
}

fn submit(u: &mut Ui, sc: &Scenario) -> Result<Value, String> {
    match sc.submit {
        Some(button) => try_click(u, sc.screen, button)?,
        None => press(u, Key::Return, Modifiers::NONE),
    }
    for &(screen, button) in sc.confirm {
        if u.screen(screen).is_some() {
            try_click(u, screen, button)?;
        }
    }
    // Anything the change itself sent (a box that sends as it is changed)
    // counts as sent, so nothing is drained between opening and here.
    Ok(snapshot(u))
}

type Observed = (Value, Option<Change>);

/// Change one control as a player would: type into a text box (`text`, or
/// a made-up value), click a checkbox or radio button, or pick another
/// dropdown entry.
fn change(u: &mut Ui, screen: ScreenId, key: &str, text: Option<&str>) -> Result<Change, String> {
    let v = view(u, screen);
    let node = v
        .walk()
        .find(|&n| key_of(v, n) == key)
        .ok_or_else(|| format!("{key} is gone"))?;
    let class = v.node(node).ctrl.class.clone();
    Ok(match class.as_str() {
        "GuiTextEditCtrl" | "GuiMLTextEditCtrl" => {
            let max = v
                .node(node)
                .ctrl
                .field("maxLength")
                .and_then(|m| m.parse().ok())
                .unwrap_or(255);
            let typed =
                text.map_or_else(|| sentinel(&v.edit_text(node), key, max), str::to_string);
            replace_text(u, screen, node, &typed)?;
            Change::Typed(typed)
        }
        "GuiCheckBoxCtrl" | "GuiRadioCtrl" => {
            let before = v.bool_value(node);
            let at = centre(u, screen, node)?;
            click_at(u, at);
            let after = view(u, screen).bool_value(node);
            if after == before && (class != "GuiRadioCtrl" || !after) {
                return Err("clicking it does not change it".into());
            }
            Change::Clicked
        }
        "GuiPopUpMenuCtrl" => {
            let before = v.selected(node);
            let entries = v.node(node).state.items.len();
            if entries < 2 {
                return Err(format!(
                    "it offers {entries} entries, so nothing else can be picked"
                ));
            }
            for step in [Key::Down, Key::Up] {
                let at = centre(u, screen, node)?;
                click_at(u, at);
                press(u, step, Modifiers::NONE);
                press(u, Key::Return, Modifiers::NONE);
                if view(u, screen).selected(node) != before {
                    break;
                }
            }
            if view(u, screen).selected(node) == before {
                return Err("picking from it does not change it".into());
            }
            Change::Picked
        }
        other => return Err(format!("no probe for {other}")),
    })
}

fn run(pack: &Rc<Pack>, sc: &Scenario, probe: Option<(&str, Option<&str>)>) -> Result<Observed, String> {
    let mut u = open(pack, sc)?;
    let change = match probe {
        Some((key, text)) => Some(change(&mut u, sc.screen, key, text)?),
        None => None,
    };
    Ok((submit(&mut u, sc)?, change))
}

/// A value for a text box that no default contains; numbers stay numbers.
fn sentinel(current: &str, key: &str, max: usize) -> String {
    let t = current.trim();
    if let Ok(n) = t.parse::<i64>() {
        return (if n < 0 { 7 } else { n + 3 }).to_string();
    }
    if t.parse::<f64>().is_ok() {
        return "7".into();
    }
    let tag: String = key
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .rev()
        .take(6)
        .collect();
    format!("Zq{tag}").chars().take(max.max(2)).collect()
}

fn controls(pack: &Rc<Pack>, sc: &Scenario) -> Result<Vec<(String, String)>, String> {
    let u = open(pack, sc)?;
    let v = view(&u, sc.screen);
    let mut out: Vec<(String, String)> = v
        .walk()
        .filter(|&n| {
            let node = v.node(n);
            // The radio button already picked is the current value; picking
            // it again changes nothing.
            let picked = node.ctrl.class == "GuiRadioCtrl" && v.bool_value(n);
            EDITABLE.contains(&node.ctrl.class.as_str())
                && v.is_shown(n)
                && node.state.active
                && !picked
        })
        .map(|n| (key_of(v, n), v.node(n).ctrl.class.clone()))
        .collect();
    out.dedup();
    Ok(out)
}

fn text_of(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn check_rows(pack: &Rc<Pack>, sc: &Scenario, rows: &Rows) -> Vec<String> {
    let mut problems = vec![];
    for &(shown, want) in rows.rows {
        let mut u = match open(pack, sc) {
            Ok(u) => u,
            Err(e) => return vec![format!("{}: {e}", sc.name)],
        };
        if let Err(e) = click_row(&mut u, sc.screen, rows.list, shown) {
            problems.push(format!("{}: row {shown:?} of {}: {e}", sc.name, rows.list));
            continue;
        }
        let mut sent = BTreeMap::new();
        match submit(&mut u, sc) {
            Ok(v) => leaves(&v, String::new(), &mut sent),
            Err(e) => {
                problems.push(format!("{}: after choosing {shown:?}: {e}", sc.name));
                continue;
            }
        }
        match sent.get(rows.path) {
            Some(v) if text_of(v) == want => {}
            got => problems.push(format!(
                "{}: choosing {shown:?} in {} sends {} = {:?}, not {want:?}",
                sc.name,
                rows.list,
                rows.path,
                got.map(text_of)
            )),
        }
    }
    problems
}

fn check(pack: &Rc<Pack>, sc: &Scenario, print: bool) -> Vec<String> {
    let mut problems = vec![];
    if let Some(rows) = &sc.rows {
        problems.extend(check_rows(pack, sc, rows));
    }
    let found = match controls(pack, sc) {
        Ok(found) => found,
        Err(e) => return vec![format!("{}: {e}", sc.name)],
    };
    if found.is_empty() && sc.expect.is_empty() {
        return problems;
    }
    let baseline = match run(pack, sc, None) {
        Ok((b, _)) => b,
        Err(e) => return vec![format!("{}: {e}", sc.name)],
    };
    let expect: BTreeMap<_, _> = sc.expect.iter().copied().collect();
    for (key, class) in &found {
        let text = match expect.get(key.as_str()) {
            Some(Expect::Typed(text, _)) => Some(*text),
            _ => None,
        };
        let observed = run(pack, sc, Some((key, text)));
        if print {
            match &observed {
                Ok((after, _)) => {
                    let paths = changed(&baseline, after);
                    println!("{} | {key} | {class} | {:?}", sc.name, paths.keys().collect::<Vec<_>>());
                    if std::env::var_os("BRI_FIELD_FLOW_VALUES").is_some() {
                        let mut was = BTreeMap::new();
                        leaves(&baseline, String::new(), &mut was);
                        for (p, v) in &paths {
                            println!("    {p}: {:?} -> {v}", was.get(p).map(text_of));
                        }
                    }
                }
                Err(e) => println!("{} | {key} | {class} | ERROR {e}", sc.name),
            }
        }
        let (paths, change) = match (observed, expect.get(key.as_str())) {
            (Err(_), Some(Expect::Blocked(_))) => continue,
            (Ok(_), Some(Expect::Blocked(why))) => {
                problems.push(format!(
                    "{}: {key} is listed as out of reach ({why}) but a player can change it",
                    sc.name
                ));
                continue;
            }
            (Ok((after, change)), _) => (changed(&baseline, &after), change),
            (Err(e), _) => {
                problems.push(format!("{}: {key} ({class}): {e}", sc.name));
                continue;
            }
        };
        let got: BTreeSet<String> = paths.keys().cloned().collect();
        let want = match expect.get(key.as_str()) {
            None => {
                problems.push(format!(
                    "{}: {key} ({class}) is on screen but not in the table; it changes {got:?}",
                    sc.name
                ));
                continue;
            }
            Some(Expect::NotSent(_)) if paths.is_empty() => continue,
            Some(Expect::NotSent(why)) => {
                problems.push(format!(
                    "{}: {key} is listed as not sent ({why}) but changes {got:?}",
                    sc.name
                ));
                continue;
            }
            Some(Expect::Blocked(_)) => unreachable!(),
            Some(Expect::Sent(want) | Expect::Typed(_, want)) => want,
        };
        let want: BTreeSet<String> = want.iter().map(|s| s.to_string()).collect();
        if want != got {
            problems.push(format!(
                "{}: {key} should change {want:?} but changes {got:?}",
                sc.name
            ));
            continue;
        }
        if let Some(Change::Typed(typed)) = &change {
            for (path, value) in &paths {
                let shown = text_of(value);
                // Words typed on purpose may be split into fields (a chat
                // command's name and arguments).
                let same = shown.contains(typed.as_str())
                    || text.is_some() && !shown.is_empty() && typed.contains(shown.as_str())

                    || shown
                        .parse::<f64>()
                        .ok()
                        .is_some_and(|n| Some(n) == typed.parse().ok());
                if !same && !matches!(value, Value::Bool(_)) {
                    problems.push(format!(
                        "{}: typed {typed:?} into {key} but {path} is {shown:?}",
                        sc.name
                    ));
                }
            }
        }
    }
    for key in expect.keys() {
        if !found.iter().any(|(k, _)| k == key) {
            problems.push(format!(
                "{}: {key} is in the table but not on screen",
                sc.name
            ));
        }
    }
    problems
}

// ----------------------------------------------------------- game state

const BEDROOM: &str = "v20/add-ons/map_bedroom/bedroom.mis";
const SLATE: &str = "v20/add-ons/map_slate/slate.mis";

fn maps() -> UiUpdate {
    UiUpdate::Maps(
        [("Bedroom", BEDROOM), ("Slate", SLATE)]
            .map(|(name, id)| MapInfo {
                id: id.into(),
                name: name.into(),
                description: format!("{name} map"),
                preview: IconRef::None,
            })
            .to_vec(),
    )
}

fn in_game(u: &mut Ui, admin: bool, single_player: bool) {
    u.apply(UiUpdate::Connection(ConnectionState::InGame {
        server_name: "Field flow".into(),
        max_players: 8,
        local: true,
        single_player,
        admin,
    }));
    u.update(0);
}

fn admin_state(u: &mut Ui, role: AdminRole) {
    u.apply(UiUpdate::Admin(AdminUpdate::State(AdminSnapshot {
        revision: 1,
        role,
        local_host: true,
        legacy_lan: false,
        supported: [
            AdminFeature::Login,
            AdminFeature::Kick,
            AdminFeature::Ban,
            AdminFeature::Unban,
            AdminFeature::Spy,
            AdminFeature::Wand,
            AdminFeature::Maps,
            AdminFeature::ClearBricks,
            AdminFeature::HostOptions,
            AdminFeature::AdminPassword,
        ]
        .into_iter()
        .collect(),
        players: vec![
            AdminPlayer {
                connection: 1,
                name: "Hosty".into(),
                identity_label: "host".into(),
                role,
                owner: true,
                local: true,
                bot: false,
                persistent_identity: true,
            },
            AdminPlayer {
                connection: 7,
                name: "Guesty".into(),
                identity_label: "guest".into(),
                role: AdminRole::Player,
                owner: false,
                local: false,
                bot: false,
                persistent_identity: true,
            },
            AdminPlayer {
                connection: 9,
                name: "Walker".into(),
                identity_label: "walker".into(),
                role: AdminRole::Player,
                owner: false,
                local: false,
                bot: false,
                persistent_identity: true,
            },
        ],
        options: Some(AdminOptions {
            name: "Field flow".into(),
            ..bri_ui::models::admin::options_from_prefs(&Default::default())
        }),
    })));
    u.update(0);
}

fn datablocks(u: &mut Ui) {
    let menu = |class: &str, names: &[&str]| {
        (
            class.to_string(),
            names
                .iter()
                .map(|n| Choice {
                    id: format!("v20/{class}/{n}").to_ascii_lowercase(),
                    name: n.to_string(),
                })
                .collect::<Vec<_>>(),
        )
    };
    u.apply(UiUpdate::Datablocks(
        [
            menu("FxLightData", &["Red Light", "Blue Light"]),
            menu("ParticleEmitterData", &["Fire", "Smoke"]),
            menu("ItemData", &["Hammer", "Gun"]),
            menu("AudioProfile", &["Beep", "Honk"]),
            menu("Music", &["Bass 1", "Rock"]),
            menu("Vehicle", &["Jeep", "Horse"]),
        ]
        .into_iter()
        .collect(),
    ));
    u.update(0);
}

fn open_wrench(u: &mut Ui, variant: WrenchVariant) {
    in_game(u, false, false);
    datablocks(u);
    u.apply(UiUpdate::OpenWrench {
        brick: 42,
        variant,
        owner: "Hosty".into(),
        data: WrenchData {
            name: String::new(),
            item_dir: 2,
            raycasting: true,
            colliding: true,
            rendering: true,
            ..Default::default()
        },
        admin_override: false,
        events_allowed: true,
    });
    u.update(0);
}

/// The events dialog of a brick with one finished row, opened from the
/// wrench's Events button with every stock event supported.
fn open_events(u: &mut Ui) {
    open_wrench(u, WrenchVariant::Normal);
    let tables = u.core.pack.data.data.event_tables.clone();
    let inputs: Vec<&str> = tables.inputs.iter().map(|i| i.name.as_str()).collect();
    let outputs: Vec<(&str, &str)> = tables
        .outputs
        .iter()
        .map(|o| (o.class.as_str(), o.name.as_str()))
        .collect();
    u.apply(UiUpdate::Events(EventCatalog::from_capabilities(
        &tables, &inputs, &outputs,
    )));
    u.apply(UiUpdate::Colorset(vec![PaintDivision {
        name: "Standard".into(),
        colors: vec![
            [1.0, 0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0, 1.0],
            [0.0, 1.0, 0.0, 1.0],
        ],
    }]));
    u.update(0);
    click(
        u,
        ScreenId::Wrench(WrenchVariant::Normal),
        "canvas.pushDialog(WrenchEventsDlg);",
    );
    answer_all(u);
    u.apply(UiUpdate::OpenEvents {
        brick: 42,
        rows: vec![EventRow::Editable(EventLine {
            enabled: true,
            delay_ms: 0,
            input: "onActivate".into(),
            target: "Self".into(),
            named_target: None,
            output: "setColor".into(),
            params: vec![ParamValue::PaintColor(1)],
        })],
        named_targets: vec!["door".into()],
        allow_named: true,
    });
    u.update(0);
}

fn minigame_state(u: &mut Ui) {
    let caps = MiniGameCapabilities {
        list: true,
        create: true,
        configure: true,
        join: true,
        leave: true,
        invite: true,
        respond_invite: true,
        remove_member: true,
        reset: true,
        respawn_all: true,
        end: true,
        scoreboard: true,
    };
    let choice = |id: &str, name: &str| MiniGameChoice {
        id: id.into(),
        name: name.into(),
    };
    u.apply(UiUpdate::MiniGames(MiniGameUiState {
        ready: true,
        revision: 1,
        capabilities: caps,
        colors: ["Red", "Blue", "Green"]
            .iter()
            .enumerate()
            .map(|(i, n)| MiniGameColor {
                index: i as u8,
                name: n.to_string(),
                rgb: [i as u8 * 60, 0, 0],
            })
            .collect(),
        local_player: Some(MiniGamePlayerId(1)),
        player_types: vec![
            choice("v20.player.playerstandardarmor", "Standard Player"),
            choice("v20.player.playernojet", "No-Jet Player"),
        ],
        items: [
            "hammeritem",
            "wrenchitem",
            "printgun",
            "gunitem",
            "rocketlauncheritem",
            "bowitem",
        ]
        .map(|i| choice(&format!("v20.weapon.{i}"), i))
        .to_vec(),
        ..Default::default()
    }));
    u.update(0);
}

/// Answer every request the screens made so far, as a host that accepted
/// them would.
fn answer_all(u: &mut Ui) {
    for (id, _) in u.drain_actions() {
        u.apply(UiUpdate::ActionResult { id, result: Ok(()) });
    }
    u.update(0);
}

/// The request id of the pending admin query `matches` names.
fn admin_query(u: &Ui, matches: fn(&bri_ui::models::admin::AdminAction) -> bool) -> RequestId {
    *u.core
        .admin
        .pending
        .iter()
        .find(|(_, a)| matches(a))
        .expect("the screen asked the host for its list")
        .0
}

fn open_admin(u: &mut Ui, screen: ScreenId) {
    in_game(u, true, false);
    admin_state(u, AdminRole::SuperAdmin);
    u.core.push(ScreenId::Admin);
    u.update(0);
    if screen != ScreenId::Admin {
        u.core.push(screen);
        u.update(0);
    }
}

fn players(u: &mut Ui) {
    let row = |id: u64, name: &str, trust: &str| PlayerRow {
        id,
        ignoring: false,
        name: name.into(),
        score: 0,
        admin: false,
        super_admin: false,
        bl_id: Some(id * 1000),
        trust: trust.into(),
    };
    u.apply(UiUpdate::Players {
        rows: vec![
            row(1, "Hosty", "You"),
            row(7, "Guesty", "-"),
            row(9, "Walker", "-"),
        ],
        server_name: "Field flow".into(),
        max_players: 8,
    });
    u.update(0);
}

fn bricks(u: &mut Ui) {
    let brick = |id: &str, name: &str| BrickInfo {
        id: id.into(),
        ui_name: name.into(),
        category: "Bricks".into(),
        subcategory: "Basic".into(),
        icon: IconRef::None,
    };
    u.apply(UiUpdate::Bricks(vec![
        brick("v20/brick/brick1x1data", "1x1"),
        brick("v20/brick/brick2x4data", "2x4"),
        brick("v20/brick/brick1x4x5windowdata", "1x4x5 Window"),
    ]));
    u.update(0);
}

const SINGLE_PLAYER: &str =
    "single player greys the server options out behind SM_OptionsBlocker, as v20";
const COPY: &str = "Copy keeps this field for the next brick wrenched; see copy_locks_keep_their_own_field";

const UNFINISHED: &str = "a row without an output is not sent; see an_events_row_built_by_clicks";
const ROW_DROPPED: &[&str] = &[
    "actions.SendEvents.rows[0].Editable.delay_ms",
    "actions.SendEvents.rows[0].Editable.enabled",
    "actions.SendEvents.rows[0].Editable.input",
    "actions.SendEvents.rows[0].Editable.named_target",
    "actions.SendEvents.rows[0].Editable.output",
    "actions.SendEvents.rows[0].Editable.params[0].PaintColor",
    "actions.SendEvents.rows[0].Editable.target",
];

fn scenarios() -> Vec<Scenario> {
    use Expect::*;
    vec![
        Scenario {
            name: "Start Game (single player)",
            open: |u| {
                u.apply(maps());
                u.core.push(ScreenId::StartMission);
                u.update(0);
            },
            screen: ScreenId::StartMission,
            submit: Some("SM_StartMission();"),
            confirm: &[],
            expect: &[
            ("TxtServerName", Blocked(SINGLE_PLAYER)),
            ("TxtServerAdminPasswordCRAP", Blocked(SINGLE_PLAYER)),
            ("$Pref::Server::SuperAdminPassword", Blocked(SINGLE_PLAYER)),
            ("SM_PlayerCountMenu", Blocked(SINGLE_PLAYER)),
            ("SM_OptInternet", Sent(&["actions.HostGame.max_players", "actions.HostGame.mode", "settings.prefs.$Pref::Net::ServerType", "settings.prefs.$Pref::Server::MaxPlayers"])),
            ("SM_OptLAN", Sent(&["actions.HostGame.max_players", "actions.HostGame.mode", "settings.prefs.$Pref::Net::ServerType", "settings.prefs.$Pref::Server::MaxPlayers"])),
            ],
            rows: Some(Rows {
                list: "SM_missionList",
                path: "actions.HostGame.map",
                rows: &[("Bedroom", BEDROOM), ("Slate", SLATE)],
            }),
        },
        Scenario {
            name: "Start Game (LAN)",
            open: |u| {
                u.apply(maps());
                u.core.push(ScreenId::StartMission);
                u.update(0);
                click(u, ScreenId::StartMission, "SM_OptLAN");
            },
            screen: ScreenId::StartMission,
            submit: Some("SM_StartMission();"),
            confirm: &[],
            expect: &[
            ("TxtServerName", Sent(&["actions.HostGame.server_name", "settings.prefs.$Pref::Server::Name"])),
            ("TxtServerAdminPasswordCRAP", Sent(&["actions.HostGame.admin_password", "settings.prefs.$Pref::Server::AdminPassword"])),
            ("$Pref::Server::SuperAdminPassword", Sent(&["actions.HostGame.super_admin_password", "settings.prefs.$Pref::Server::SuperAdminPassword"])),
            ("SM_PlayerCountMenu", Sent(&["actions.HostGame.max_players", "settings.prefs.$Pref::Server::MaxPlayers"])),
            ("SM_OptInternet", Sent(&["actions.HostGame.mode", "settings.prefs.$Pref::Net::ServerType"])),
            ("SM_OptSinglePlayer", Sent(&["actions.HostGame.max_players", "actions.HostGame.mode", "settings.prefs.$Pref::Net::ServerType", "settings.prefs.$Pref::Server::MaxPlayers"])),
            ],
            rows: None,
        },
        Scenario {
            name: "Advanced Config (saved)",
            open: |u| {
                u.apply(maps());
                u.core.push(ScreenId::StartMission);
                u.update(0);
                click(u, ScreenId::StartMission, "canvas.pushDialog(ServerconfigGui);");
            },
            screen: ScreenId::ServerConfig,
            submit: Some("canvas.popDialog(ServerConfigGui);"),
            confirm: &[],
            expect: &[
            ("AdminOption_port", Sent(&["settings.prefs.$Pref::Server::Port"])),
            ("AdminOption_bricklimit", Sent(&["settings.prefs.$Pref::Server::BrickLimit"])),
            ("AdminOption_maxbrickspersecond", Sent(&["settings.prefs.$Pref::Server::MaxBricksPerSecond"])),
            ("AdminOption_randombrickcolor", Sent(&["settings.prefs.$Pref::Server::RandomBrickColor"])),
            ("AdminOption_maxchatlen", Sent(&["settings.prefs.$Pref::Server::MaxChatLen"])),
            ("AdminOption_quota::schedules", Sent(&["settings.prefs.$Pref::Server::Quota::Schedules"])),
            ("AdminOption_maxphysvehicles_total", Sent(&["settings.prefs.$Pref::Server::MaxPhysVehicles_Total"])),
            ("AdminOption_quota::misc", Sent(&["settings.prefs.$Pref::Server::Quota::Misc"])),
            ("AdminOption_quota::projectile", Sent(&["settings.prefs.$Pref::Server::Quota::Projectile"])),
            ("AdminOption_etardfilter", Sent(&["settings.prefs.$Pref::Server::ETardFilter"])),
            ("AdminOption_brickpublicdomaintimeout", Sent(&["settings.prefs.$Pref::Server::BrickPublicDomainTimeout"])),
            ("AdminOption_fallingdamage", Sent(&["settings.prefs.$Pref::Server::FallingDamage"])),
            ("AdminOption_maxplayervehicles_total", Sent(&["settings.prefs.$Pref::Server::MaxPlayerVehicles_Total"])),
            ("AdminOption_quota::item", Sent(&["settings.prefs.$Pref::Server::Quota::Item"])),
            ("AdminOption_quota::vehicle", Sent(&["settings.prefs.$Pref::Server::Quota::Vehicle"])),
            ("AdminOption_quota::player", Sent(&["settings.prefs.$Pref::Server::Quota::Player"])),
            ("AdminOption_quota::environment", Sent(&["settings.prefs.$Pref::Server::Quota::Environment"])),
            ("AdminOption_quotalan::vehicle", Sent(&["settings.prefs.$Pref::Server::QuotaLAN::Vehicle"])),
            ("AdminOption_quotalan::player", Sent(&["settings.prefs.$Pref::Server::QuotaLAN::Player"])),
            ("AdminOption_quotalan::environment", Sent(&["settings.prefs.$Pref::Server::QuotaLAN::Environment"])),
            ("AdminOption_quotalan::item", Sent(&["settings.prefs.$Pref::Server::QuotaLAN::Item"])),
            ("AdminOption_quotalan::projectile", Sent(&["settings.prefs.$Pref::Server::QuotaLAN::Projectile"])),
            ("AdminOption_quotalan::misc", Sent(&["settings.prefs.$Pref::Server::QuotaLAN::Misc"])),
            ("AdminOption_quotalan::schedules", Sent(&["settings.prefs.$Pref::Server::QuotaLAN::Schedules"])),
            ("AdminOption_toofardistance", Sent(&["settings.prefs.$Pref::Server::TooFarDistance"])),
            ],
            rows: None,
        },
        Scenario {
            name: "Join Server",
            open: |u| {
                let server = |address: &str, name: &str| ServerInfo {
                    address: address.into(),
                    name: name.into(),
                    password: false,
                    dedicated: false,
                    ping_ms: Some(5),
                    players: 1,
                    max_players: 8,
                    bricks: 0,
                    map: "Bedroom".into(),
                    favorite: false,
                };
                u.core.push(ScreenId::JoinServer);
                u.update(0);
                u.apply(UiUpdate::LanServers {
                    servers: vec![
                        server("192.168.1.20:28000", "Alpha builds"),
                        server("192.168.1.30:28001", "Beta deathmatch"),
                    ],
                    querying: false,
                });
                u.update(0);
            },
            screen: ScreenId::JoinServer,
            submit: Some("JoinServerGui.join();"),
            confirm: &[],
            expect: &[],
            rows: Some(Rows {
                list: "JS_serverList",
                path: "actions.JoinServer.address",
                rows: &[
                    ("Alpha builds", "192.168.1.20:28000"),
                    ("Beta deathmatch", "192.168.1.30:28001"),
                ],
            }),
        },
        Scenario {
            name: "Connect to IP",
            open: |u| {
                u.core.prefs.set("$pref::Join::Address", "10.0.0.5:28000");
                u.core.push(ScreenId::ManualJoin);
                u.update(0);
            },
            screen: ScreenId::ManualJoin,
            submit: Some("MJ_connect();"),
            confirm: &[],
            expect: &[
            ("MJ_txtIP", Sent(&["actions.JoinServer.address", "settings.prefs.$pref::Join::Address"])),
            ],
            rows: None,
        },
        Scenario {
            name: "Avatar",
            open: |u| {
                u.core.push(ScreenId::Avatar);
                u.update(0);
            },
            screen: ScreenId::Avatar,
            submit: Some("Avatar_Done();"),
            confirm: &[],
            expect: &[
            ("Avatar_Prefix", Sent(&["actions.SetAvatar.clan_prefix"])),
            ("Avatar_Suffix", Sent(&["actions.SetAvatar.clan_suffix"])),
            ("Avatar_SymmetryCheckbox", Sent(&["actions.SetAvatar.symmetry"])),
            ("Avatar_Name", Sent(&["actions.SetAvatar.lan_name"])),
            ],
            rows: None,
        },
        Scenario {
            name: "Choose Name",
            open: |u| {
                u.core.push(ScreenId::ChooseName);
                u.update(0);
            },
            screen: ScreenId::ChooseName,
            submit: Some("regNameGui::register();"),
            confirm: &[],
            expect: &[
            ("regName_NewName", Sent(&["settings.avatar.lan_name", "settings.prefs.$pref::Player::LANName"])),
            ],
            rows: None,
        },
        Scenario {
            name: "Save Bricks",
            open: |u| {
                in_game(u, true, true);
                u.apply(UiUpdate::SaveContext {
                    map: "Bedroom".into(),
                    preview: IconRef::None,
                });
                u.core.push(ScreenId::SaveBricks);
                u.update(0);
                u.apply(UiUpdate::SaveFiles {
                    maps: vec!["Bedroom".into()],
                    files: vec![],
                });
                u.update(0);
                click(u, ScreenId::SaveBricks, "SaveBricks_FileName");
                type_text(u, "House");
            },
            screen: ScreenId::SaveBricks,
            submit: Some("SaveBricks_Save();"),
            confirm: &[],
            expect: &[
            ("SaveBricks_FileName", Sent(&["actions.SaveBricks.name"])),
            ("SaveBricks_Description", Sent(&["actions.SaveBricks.description"])),
            ("SaveBricks_ExtendedInfo", Sent(&["actions.SaveBricks.events"])),
            ("SaveBricks_Ownership", Sent(&["actions.SaveBricks.ownership"])),
            ],
            rows: None,
        },
        Scenario {
            name: "Load Bricks",
            open: |u| {
                in_game(u, true, true);
                u.apply(UiUpdate::SaveContext {
                    map: "Bedroom".into(),
                    preview: IconRef::None,
                });
                u.core.push(ScreenId::LoadBricks);
                u.update(0);
                let file = |name: &str, map: &str| SaveFileInfo {
                    name: name.into(),
                    map: map.into(),
                    modified: "2026-09-28 10:00".into(),
                    description: format!("{name} build"),
                    brick_count: Some(10),
                    damaged: false,
                };
                u.apply(UiUpdate::SaveFiles {
                    maps: vec!["Bedroom".into(), "Slate".into()],
                    files: vec![
                        file("Castle.world.json", "Bedroom"),
                        file("Tower.world.json", "Bedroom"),
                        file("Bridge.world.json", "Slate"),
                    ],
                });
                u.update(0);
                click_row(u, ScreenId::LoadBricks, "LoadBricks_FileList", "Castle").unwrap();
            },
            screen: ScreenId::LoadBricks,
            submit: Some("LoadBricks_ClickLoadButton();"),
            confirm: &[],
            expect: &[
            ("LoadBricks_MapMenu", Sent(&["actions.LoadBricks.map", "actions.LoadBricks.name", "actions.LoadBricks.ownership", "actions.RequestSaveList.map"])),
            ("LoadBricks_DoOwnership", Sent(&["actions.LoadBricks.ownership"])),
            // Finds and picks the best match, here another save on the same map.
            ("LoadBricks_Search", Typed("Tower", &["actions.LoadBricks.name"])),
            ],
            rows: Some(Rows {
                list: "LoadBricks_FileList",
                path: "actions.LoadBricks.name",
                rows: &[
                    ("Castle", "Castle.world.json"),
                    ("Tower", "Tower.world.json"),
                ],
            }),
        },
        Scenario {
            name: "Brick Selector search",
            open: |u| {
                in_game(u, false, false);
                bricks(u);
                u.core.push(ScreenId::BrickSelector);
                u.update(0);
                click(u, ScreenId::BrickSelector, "BSD_Search");
                type_text(u, "1x1");
            },
            screen: ScreenId::BrickSelector,
            submit: None,
            confirm: &[],
            expect: &[
            ("BSD_Search", Typed("2x4", &["actions.InstantUseBrick.brick"])),
            ],
            rows: None,
        },
        Scenario {
            name: "Player List",
            open: |u| {
                in_game(u, false, false);
                players(u);
                u.core.push(ScreenId::PlayerList);
                u.update(0);
            },
            screen: ScreenId::PlayerList,
            submit: Some("NewPlayerListGui.clickTrustInviteFull();"),
            confirm: &[],
            expect: &[],
            rows: Some(Rows {
                list: "NPL_List",
                path: "actions.TrustInvite.target",
                rows: &[("Guesty", "7"), ("Walker", "9")],
            }),
        },
        Scenario {
            name: "Join Mini-Game",
            open: |u| {
                in_game(u, false, false);
                minigame_state(u);
                let mut state = u.core.minigames.clone();
                let game = |id: u64, title: &str, owner: &str| MiniGameSummary {
                    id: MiniGameId(id),
                    title: title.into(),
                    owner: MiniGamePlayerId(id + 100),
                    owner_name: owner.into(),
                    color: 0,
                    member_count: 1,
                    invite_only: false,
                    rules: MiniGameRules::default(),
                    teams: Vec::new(),
                    addon_settings: Default::default(),
                    default: false,
                    paint_color: None,
                    members: Vec::new(),
                };
                state.games = vec![game(3, "Castle Wars", "Guesty"), game(5, "Race", "Walker")];
                state.revision += 1;
                u.apply(UiUpdate::MiniGames(state));
                u.core.push(ScreenId::MiniGames);
                u.update(0);
                answer_all(u);
            },
            screen: ScreenId::MiniGames,
            submit: Some("JoinMiniGameGui.clickJoin();"),
            confirm: &[],
            expect: &[],
            rows: Some(Rows {
                list: "JMG_List",
                path: "actions.JoinMiniGame.game",
                rows: &[("Castle Wars", "3"), ("Race", "5")],
            }),
        },
        Scenario {
            name: "Create Mini-Game",
            open: |u| {
                in_game(u, false, false);
                minigame_state(u);
                u.core.push(ScreenId::MiniGameSettings);
                u.update(0);
            },
            screen: ScreenId::MiniGameSettings,
            submit: Some("CreateMiniGameGui.clickCreate();"),
            confirm: &[],
            expect: &[
            ("$MiniGame::Title", Sent(&["actions.CreateMiniGame.rules.title"])),
            ("CMG_ColorList", Sent(&["actions.CreateMiniGame.color"])),
            ("$MiniGame::PlayersUseOwnBricks", Sent(&["actions.CreateMiniGame.rules.players_use_own_bricks"])),
            ("$MiniGame::UseAllPlayersBricks", Sent(&["actions.CreateMiniGame.rules.use_all_players_bricks"])),
            ("$MiniGame::InviteOnly", Sent(&["actions.CreateMiniGame.rules.invite_only"])),
            ("$MiniGame::Points::BreakBrick", Sent(&["actions.CreateMiniGame.rules.points_break_brick"])),
            ("$MiniGame::Points::PlantBrick", Sent(&["actions.CreateMiniGame.rules.points_plant_brick"])),
            ("$MiniGame::Points::KillPlayer", Sent(&["actions.CreateMiniGame.rules.points_kill_player"])),
            ("$MiniGame::Points::KillSelf", Sent(&["actions.CreateMiniGame.rules.points_kill_self"])),
            ("$MiniGame::Points::Die", Sent(&["actions.CreateMiniGame.rules.points_die"])),
            ("$MiniGame::BrickRespawnTime", Sent(&["actions.CreateMiniGame.rules.brick_respawn_seconds"])),
            ("$MiniGame::VehicleRespawnTime", Sent(&["actions.CreateMiniGame.rules.vehicle_respawn_seconds"])),
            ("$MiniGame::RespawnTime", Sent(&["actions.CreateMiniGame.rules.respawn_seconds"])),
            ("$MiniGame::BrickDamage", Sent(&["actions.CreateMiniGame.rules.brick_damage"])),
            ("$MiniGame::VehicleDamage", Sent(&["actions.CreateMiniGame.rules.vehicle_damage"])),
            ("$MiniGame::SelfDamage", Sent(&["actions.CreateMiniGame.rules.self_damage"])),
            ("$MiniGame::WeaponDamage", Sent(&["actions.CreateMiniGame.rules.weapon_damage"])),
            ("$MiniGame::FallingDamage", Sent(&["actions.CreateMiniGame.rules.falling_damage"])),
            ("$MiniGame::UseSpawnBricks", Sent(&["actions.CreateMiniGame.rules.use_spawn_bricks"])),
            ("CMG_PlayerDataBlock", Sent(&["actions.CreateMiniGame.rules.player_type"])),
            ("$MiniGame::EnableWand", Sent(&["actions.CreateMiniGame.rules.enable_wand"])),
            ("$MiniGame::EnableBuilding", Sent(&["actions.CreateMiniGame.rules.enable_building"])),
            ("$MiniGame::EnablePainting", Sent(&["actions.CreateMiniGame.rules.enable_painting"])),
            ("CMG_StartEquip4", Sent(&["actions.CreateMiniGame.rules.loadout[4]"])),
            ("CMG_StartEquip3", Sent(&["actions.CreateMiniGame.rules.loadout[3]"])),
            ("CMG_StartEquip2", Sent(&["actions.CreateMiniGame.rules.loadout[2]"])),
            ("CMG_StartEquip1", Sent(&["actions.CreateMiniGame.rules.loadout[1]"])),
            ("CMG_StartEquip0", Sent(&["actions.CreateMiniGame.rules.loadout[0]"])),
            ],
            rows: None,
        },
        Scenario {
            name: "Wrench",
            open: |u| open_wrench(u, WrenchVariant::Normal),
            screen: ScreenId::Wrench(WrenchVariant::Normal),
            submit: Some("wrenchDlg.send();"),
            confirm: &[],
            expect: &[
            ("Wrench_Name", Sent(&["actions.SendWrench.data.name"])),
            ("Wrench_Lights", Sent(&["actions.SendWrench.data.light"])),
            ("Wrench_Emitters", Sent(&["actions.SendWrench.data.emitter"])),
            ("WrenchLock_Lights", NotSent(COPY)),
            ("WrenchLock_Name", NotSent(COPY)),
            ("WrenchLock_Emitters", NotSent(COPY)),
            ("Wrench_EmitterDir1", Sent(&["actions.SendWrench.data.emitter_dir"])),
            ("Wrench_EmitterDir2", Sent(&["actions.SendWrench.data.emitter_dir"])),
            ("Wrench_EmitterDir3", Sent(&["actions.SendWrench.data.emitter_dir"])),
            ("Wrench_EmitterDir4", Sent(&["actions.SendWrench.data.emitter_dir"])),
            ("Wrench_EmitterDir5", Sent(&["actions.SendWrench.data.emitter_dir"])),
            ("WrenchLock_EmitterDir", NotSent(COPY)),
            ("Wrench_Items", Sent(&["actions.SendWrench.data.item"])),
            ("WrenchLock_Items", NotSent(COPY)),
            ("Wrench_ItemPos1", Sent(&["actions.SendWrench.data.item_pos"])),
            ("Wrench_ItemPos2", Sent(&["actions.SendWrench.data.item_pos"])),
            ("Wrench_ItemPos3", Sent(&["actions.SendWrench.data.item_pos"])),
            ("Wrench_ItemPos4", Sent(&["actions.SendWrench.data.item_pos"])),
            ("Wrench_ItemPos5", Sent(&["actions.SendWrench.data.item_pos"])),
            ("WrenchLock_ItemPos", NotSent(COPY)),
            ("Wrench_ItemDir5", Sent(&["actions.SendWrench.data.item_dir"])),
            ("WrenchLock_ItemDir", NotSent(COPY)),
            ("Wrench_ItemDir3", Sent(&["actions.SendWrench.data.item_dir"])),
            ("Wrench_ItemDir4", Sent(&["actions.SendWrench.data.item_dir"])),
            ("WrenchLock_ItemRespawnTime", NotSent(COPY)),
            ("Wrench_ItemRespawnTime", Sent(&["actions.SendWrench.data.item_respawn_ms"])),
            ("WrenchLock_RayCasting", NotSent(COPY)),
            ("Wrench_RayCasting", Sent(&["actions.SendWrench.data.raycasting"])),
            ("Wrench_Collision", Sent(&["actions.SendWrench.data.colliding"])),
            ("WrenchLock_Collision", NotSent(COPY)),
            ("WrenchLock_Rendering", NotSent(COPY)),
            ("Wrench_Rendering", Sent(&["actions.SendWrench.data.rendering"])),
            ],
            rows: None,
        },
        Scenario {
            name: "Wrench (sound)",
            open: |u| open_wrench(u, WrenchVariant::Sound),
            screen: ScreenId::Wrench(WrenchVariant::Sound),
            submit: Some("wrenchSoundDlg.send();"),
            confirm: &[],
            expect: &[
            ("WrenchSound_Sounds", Sent(&["actions.SendWrench.data.sound"])),
            ("WrenchSoundLock_Sounds", NotSent(COPY)),
            ("WrenchSound_Name", Sent(&["actions.SendWrench.data.name"])),
            ("WrenchSoundLock_Name", NotSent(COPY)),
            ],
            rows: None,
        },
        Scenario {
            name: "Wrench (vehicle spawn)",
            open: |u| open_wrench(u, WrenchVariant::VehicleSpawn),
            screen: ScreenId::Wrench(WrenchVariant::VehicleSpawn),
            submit: Some("wrenchVehicleSpawnDlg.send();"),
            confirm: &[],
            expect: &[
            ("WrenchVehicleSpawn_Vehicles", Sent(&["actions.SendWrench.data.vehicle"])),
            ("WrenchVehicleSpawnLock_Vehicles", NotSent(COPY)),
            ("WrenchVehicleSpawn_Name", Sent(&["actions.SendWrench.data.name"])),
            ("WrenchVehicleSpawnLock_Name", NotSent(COPY)),
            ("WrenchVehicleSpawnLock_ReColorVehicle", NotSent(COPY)),
            ("WrenchVehicleSpawn_ReColorVehicle", Sent(&["actions.SendWrench.data.recolor_vehicle"])),
            ("WrenchVehicleSpawn_Rendering", Sent(&["actions.SendWrench.data.rendering"])),
            ("WrenchVehicleSpawn_Collision", Sent(&["actions.SendWrench.data.colliding"])),
            ("WrenchVehicleSpawn_RayCasting", Sent(&["actions.SendWrench.data.raycasting"])),
            ("WrenchVehicleSpawnLock_RayCasting", NotSent(COPY)),
            ("WrenchVehicleSpawnLock_Collision", NotSent(COPY)),
            ("WrenchVehicleSpawnLock_Rendering", NotSent(COPY)),
            ],
            rows: None,
        },
        Scenario {
            name: "Events",
            open: open_events,
            screen: ScreenId::WrenchEvents,
            submit: Some("wrenchEventsDlg.send();"),
            confirm: &[],
            expect: &[
                ("WrenchLock_Events", NotSent(COPY)),
                ("WrenchEvent_0_enabled", Sent(&["actions.SendEvents.rows[0].Editable.enabled"])),
                ("WrenchEvent_0_delay", Sent(&["actions.SendEvents.rows[0].Editable.delay_ms"])),
                // A new input or target clears the rest of the row, which is
                // then not sent until an output is picked again (v20
                // createTargetList/createOutputList, send skips it).
                ("WrenchEvent_0_input", Sent(ROW_DROPPED)),
                ("WrenchEvent_0_target", Sent(ROW_DROPPED)),
                ("WrenchEvent_0_output", Sent(&[
                    "actions.SendEvents.rows[0].Editable.output",
                    "actions.SendEvents.rows[0].Editable.params[0].List",
                    "actions.SendEvents.rows[0].Editable.params[0].PaintColor",
                ])),
                ("WrenchEvent_0_param0", Sent(&["actions.SendEvents.rows[0].Editable.params[0].PaintColor"])),
                ("WrenchEvent_1_enabled", NotSent(UNFINISHED)),
                ("WrenchEvent_1_delay", NotSent(UNFINISHED)),
                ("WrenchEvent_1_input", NotSent(UNFINISHED)),
            ],
            rows: None,
        },
        Scenario {
            name: "Admin login",
            open: |u| {
                in_game(u, false, false);
                admin_state(u, AdminRole::Player);
                u.core.push(ScreenId::AdminLogin);
                u.update(0);
            },
            screen: ScreenId::AdminLogin,
            submit: None,
            confirm: &[],
            expect: &[
            ("txtAdminPass", Sent(&["actions.Admin.Login.password"])),
            ],
            rows: None,
        },
        Scenario {
            name: "Kick",
            open: |u| open_admin(u, ScreenId::Admin),
            screen: ScreenId::Admin,
            submit: Some("AdminGui_KickPlayer();"),
            confirm: &[(ScreenId::AdminConfirm, "YES")],
            expect: &[],
            rows: Some(Rows {
                list: "lstAdminPlayerList",
                path: "actions.Admin.Kick.target",
                rows: &[("Guesty", "7"), ("Walker", "9")],
            }),
        },
        Scenario {
            name: "Ban",
            open: |u| {
                open_admin(u, ScreenId::Admin);
                click_row(u, ScreenId::Admin, "lstAdminPlayerList", "Guesty").unwrap();
                click(u, ScreenId::Admin, "AdminGui_BanPlayer();");
                // A ban needs a length; start from one day.
                click(u, ScreenId::AdminBan, "AddBan_Days");
                press(u, Key::Down, Modifiers::NONE);
                press(u, Key::Return, Modifiers::NONE);
            },
            screen: ScreenId::AdminBan,
            submit: Some("addBanGui.ban();"),
            confirm: &[(ScreenId::AdminConfirm, "YES")],
            expect: &[
            ("AddBan_Days", Sent(&["actions.Admin.Ban.minutes"])),
            ("AddBan_Hours", Sent(&["actions.Admin.Ban.minutes"])),
            ("AddBan_Minutes", Sent(&["actions.Admin.Ban.minutes"])),
            ("AddBan_Forever", Sent(&["actions.Admin.Ban.minutes"])),
            ("addBan_reason", Sent(&["actions.Admin.Ban.reason"])),
            ],
            rows: None,
        },
        Scenario {
            name: "Un-Ban",
            open: |u| {
                open_admin(u, ScreenId::AdminUnban);
                let request = admin_query(u, |a| {
                    matches!(a, bri_ui::models::admin::AdminAction::RequestBans)
                });
                let ban = |id: u64, name: &str| bri_ui::models::admin::AdminBan {
                    id,
                    administrator: "Hosty".into(),
                    name: name.into(),
                    identity_label: name.to_lowercase(),
                    address: None,
                    reason: "griefing".into(),
                    remaining_minutes: Some(60),
                };
                u.apply(UiUpdate::Admin(AdminUpdate::Bans {
                    request,
                    revision: 1,
                    rows: vec![ban(21, "Griefer"), ban(22, "Spammer")],
                }));
                u.update(0);
            },
            screen: ScreenId::AdminUnban,
            submit: Some("unBanGui.clickUnBan();"),
            confirm: &[(ScreenId::AdminConfirm, "YES")],
            expect: &[],
            rows: Some(Rows {
                list: "unBan_list",
                path: "actions.Admin.Unban.ban",
                rows: &[("Griefer", "21"), ("Spammer", "22")],
            }),
        },
        Scenario {
            name: "Change Map",
            open: |u| {
                open_admin(u, ScreenId::AdminMaps);
                let request = admin_query(u, |a| {
                    matches!(a, bri_ui::models::admin::AdminAction::RequestMaps)
                });
                u.apply(UiUpdate::Admin(AdminUpdate::Maps {
                    request,
                    revision: 1,
                    rows: [("Bedroom", BEDROOM), ("Slate", SLATE)]
                        .map(|(name, id)| bri_ui::models::admin::AdminMap {
                            id: id.into(),
                            name: name.into(),
                        })
                        .to_vec(),
                }));
                u.update(0);
            },
            screen: ScreenId::AdminMaps,
            submit: Some("changeMapButton.click();"),
            confirm: &[(ScreenId::AdminConfirm, "YES")],
            expect: &[],
            rows: Some(Rows {
                list: "changeMapList",
                path: "actions.Admin.ChangeMap.map",
                rows: &[("Bedroom", BEDROOM), ("Slate", SLATE)],
            }),
        },
        Scenario {
            name: "Host options",
            open: |u| open_admin(u, ScreenId::AdminOptions),
            screen: ScreenId::AdminOptions,
            submit: Some("canvas.popDialog(ServerConfigGui);"),
            confirm: &[],
            expect: &[
            ("AdminOption_port", Sent(&["actions.Admin.ConfigureHost.options.port", "settings.prefs.$Pref::Server::Port"])),
            ("AdminOption_bricklimit", Sent(&["actions.Admin.ConfigureHost.options.brick_limit", "settings.prefs.$Pref::Server::BrickLimit"])),
            ("AdminOption_maxbrickspersecond", Sent(&["actions.Admin.ConfigureHost.options.bricks_per_second", "settings.prefs.$Pref::Server::MaxBricksPerSecond"])),
            ("AdminOption_randombrickcolor", Sent(&["actions.Admin.ConfigureHost.options.random_brick_color", "settings.prefs.$Pref::Server::RandomBrickColor"])),
            ("AdminOption_maxchatlen", Sent(&["actions.Admin.ConfigureHost.options.max_chat_length", "settings.prefs.$Pref::Server::MaxChatLen"])),
            ("AdminOption_quota::schedules", Sent(&["actions.Admin.ConfigureHost.options.per_player.schedules", "settings.prefs.$Pref::Server::Quota::Schedules"])),
            ("AdminOption_maxphysvehicles_total", Sent(&["actions.Admin.ConfigureHost.options.physics_vehicles", "settings.prefs.$Pref::Server::MaxPhysVehicles_Total"])),
            ("AdminOption_quota::misc", Sent(&["actions.Admin.ConfigureHost.options.per_player.misc", "settings.prefs.$Pref::Server::Quota::Misc"])),
            ("AdminOption_quota::projectile", Sent(&["actions.Admin.ConfigureHost.options.per_player.projectiles", "settings.prefs.$Pref::Server::Quota::Projectile"])),
            ("AdminOption_etardfilter", Sent(&["actions.Admin.ConfigureHost.options.chat_filter", "settings.prefs.$Pref::Server::ETardFilter"])),
            ("AdminOption_brickpublicdomaintimeout", Sent(&["actions.Admin.ConfigureHost.options.public_domain_timeout_minutes", "settings.prefs.$Pref::Server::BrickPublicDomainTimeout"])),
            ("AdminOption_fallingdamage", Sent(&["actions.Admin.ConfigureHost.options.falling_damage", "settings.prefs.$Pref::Server::FallingDamage"])),
            ("AdminOption_maxplayervehicles_total", Sent(&["actions.Admin.ConfigureHost.options.player_vehicles", "settings.prefs.$Pref::Server::MaxPlayerVehicles_Total"])),
            ("AdminOption_quota::item", Sent(&["actions.Admin.ConfigureHost.options.per_player.items", "settings.prefs.$Pref::Server::Quota::Item"])),
            ("AdminOption_quota::vehicle", Sent(&["actions.Admin.ConfigureHost.options.per_player.vehicles", "settings.prefs.$Pref::Server::Quota::Vehicle"])),
            ("AdminOption_quota::player", Sent(&["actions.Admin.ConfigureHost.options.per_player.players", "settings.prefs.$Pref::Server::Quota::Player"])),
            ("AdminOption_quota::environment", Sent(&["actions.Admin.ConfigureHost.options.per_player.environment", "settings.prefs.$Pref::Server::Quota::Environment"])),
            ("AdminOption_quotalan::vehicle", Sent(&["actions.Admin.ConfigureHost.options.lan.vehicles", "settings.prefs.$Pref::Server::QuotaLAN::Vehicle"])),
            ("AdminOption_quotalan::player", Sent(&["actions.Admin.ConfigureHost.options.lan.players", "settings.prefs.$Pref::Server::QuotaLAN::Player"])),
            ("AdminOption_quotalan::environment", Sent(&["actions.Admin.ConfigureHost.options.lan.environment", "settings.prefs.$Pref::Server::QuotaLAN::Environment"])),
            ("AdminOption_quotalan::item", Sent(&["actions.Admin.ConfigureHost.options.lan.items", "settings.prefs.$Pref::Server::QuotaLAN::Item"])),
            ("AdminOption_quotalan::projectile", Sent(&["actions.Admin.ConfigureHost.options.lan.projectiles", "settings.prefs.$Pref::Server::QuotaLAN::Projectile"])),
            ("AdminOption_quotalan::misc", Sent(&["actions.Admin.ConfigureHost.options.lan.misc", "settings.prefs.$Pref::Server::QuotaLAN::Misc"])),
            ("AdminOption_quotalan::schedules", Sent(&["actions.Admin.ConfigureHost.options.lan.schedules", "settings.prefs.$Pref::Server::QuotaLAN::Schedules"])),
            ("AdminOption_toofardistance", Sent(&["actions.Admin.ConfigureHost.options.too_far_distance", "settings.prefs.$Pref::Server::TooFarDistance"])),
            ],
            rows: None,
        },
        Scenario {
            name: "Admin passwords",
            open: |u| open_admin(u, ScreenId::AdminCredentials),
            screen: ScreenId::AdminCredentials,
            submit: Some("AdminApplyPassword"),
            confirm: &[],
            expect: &[
            ("AdminServerName", NotSent("sent by Apply; see Server name and size")),
            ("AdminMaxPlayers", NotSent("sent by Apply; see Server name and size")),
            ("AdminNewPassword", Sent(&["actions.Admin.SetPassword.password"])),
            ("AdminPasswordSlot", Sent(&["actions.Admin.SetPassword.slot"])),
            ],
            rows: None,
        },
        Scenario {
            name: "Server name and size",
            open: |u| open_admin(u, ScreenId::AdminCredentials),
            screen: ScreenId::AdminCredentials,
            submit: Some("AdminApplyIdentity"),
            confirm: &[],
            expect: &[
            ("AdminServerName", Sent(&["actions.Admin.ConfigureHost.options.name"])),
            ("AdminMaxPlayers", Sent(&["actions.Admin.ConfigureHost.options.max_players"])),
            ("AdminNewPassword", NotSent("sent by Set password; see Admin passwords")),
            ("AdminPasswordSlot", NotSent("sent by Set password; see Admin passwords")),
            ],
            rows: None,
        },
        Scenario {
            name: "Chat",
            open: |u| {
                in_game(u, false, false);
                u.core.push(ScreenId::MessageInput(ChatChannel::Say));
                u.update(0);
                type_text(u, "hello");
            },
            screen: ScreenId::MessageInput(ChatChannel::Say),
            submit: None,
            confirm: &[],
            expect: &[
            ("NMH_Type", Sent(&["actions.Chat.text"])),
            ],
            rows: None,
        },
        Scenario {
            name: "Console",
            open: |u| {
                in_game(u, true, false);
                u.core.push(ScreenId::Console);
                u.update(0);
                type_text(u, "/timescale 1");
            },
            screen: ScreenId::Console,
            submit: None,
            confirm: &[],
            expect: &[
            ("ConsoleEntry", Typed("/Zqcmd Zqarg", &["actions.ChatCommand.args[0]", "actions.ChatCommand.name"])),
            ],
            rows: None,
        },
    ]
}

#[test]
fn every_entered_value_reaches_what_the_screen_sends() {
    let Some(pack) = pack() else { return };
    let print = std::env::var_os("BRI_FIELD_FLOW_PRINT").is_some();
    let mut problems = vec![];
    for sc in scenarios() {
        problems.extend(check(&pack, &sc, print));
    }
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}

/// v20's wrench "Copy" boxes: a locked field keeps what was sent for the
/// next brick wrenched, and each box locks its own field and no other.
#[test]
fn copy_locks_keep_their_own_field() {
    let Some(pack) = pack() else { return };
    let mut problems = vec![];
    for (variant, send) in [
        (WrenchVariant::Normal, "wrenchDlg.send();"),
        (WrenchVariant::Sound, "wrenchSoundDlg.send();"),
        (WrenchVariant::VehicleSpawn, "wrenchVehicleSpawnDlg.send();"),
    ] {
        let screen = ScreenId::Wrench(variant);
        let mut probe = new_ui(&pack);
        open_wrench(&mut probe, variant);
        let v = view(&probe, screen);
        let locks: Vec<String> = v
            .walk()
            .filter_map(|n| v.node(n).ctrl.name.clone())
            .filter(|name| name.contains("Lock_"))
            .collect();
        for lock in locks {
            let field = lock.replacen("Lock_", "_", 1);
            // The field a lock guards; radio groups are named by position.
            let target = v
                .walk()
                .filter_map(|n| v.node(n).ctrl.name.clone())
                .find(|name| {
                    name == &field
                        || name.starts_with(&field) && name[field.len()..].parse::<u8>().is_ok()
                })
                .filter(|name| name != &field || v.id(name).is_some());
            let Some(target) = target else {
                problems.push(format!("{lock}: no field named {field}*"));
                continue;
            };
            // A radio group's first button may already be picked.
            let target = if target.ends_with('0') && v.id(&target).is_some_and(|n| v.bool_value(n)) {
                format!("{}1", &target[..target.len() - 1])
            } else {
                target
            };
            let sent = |u: &mut Ui| -> BTreeMap<String, Value> {
                let mut out = BTreeMap::new();
                leaves(&snapshot(u)["actions"]["SendWrench"]["data"], String::new(), &mut out);
                out
            };
            let mut u = new_ui(&pack);
            open_wrench(&mut u, variant);
            u.drain_actions();
            if let Err(e) = change(&mut u, screen, &target, None) {
                problems.push(format!("{lock}: changing {target}: {e}"));
                continue;
            }
            click(&mut u, screen, &lock);
            click(&mut u, screen, send);
            let first = sent(&mut u);
            answer_all(&mut u);
            // The next brick arrives with the stock values.
            open_wrench(&mut u, variant);
            if u.top_id() != screen {
                problems.push(format!("{lock}: the second wrench did not open"));
                continue;
            }
            click(&mut u, screen, send);
            let second = sent(&mut u);
            let mut plain = new_ui(&pack);
            open_wrench(&mut plain, variant);
            plain.drain_actions();
            click(&mut plain, screen, send);
            let stock = sent(&mut plain);
            let kept: Vec<_> = first
                .iter()
                .filter(|(k, v)| stock.get(*k) != Some(*v) && second.get(*k) == Some(*v))
                .map(|(k, _)| k.clone())
                .collect();
            let leaked: Vec<_> = second
                .iter()
                .filter(|(k, v)| stock.get(*k) != Some(*v) && first.get(*k) != Some(*v))
                .map(|(k, _)| k.clone())
                .collect();
            let changed: Vec<_> = first
                .iter()
                .filter(|(k, v)| stock.get(*k) != Some(*v))
                .map(|(k, _)| k.clone())
                .collect();
            if kept.is_empty() || kept != changed || !leaked.is_empty() {
                problems.push(format!(
                    "{lock}: changed {changed:?} via {target}, the next brick kept {kept:?} (other changes {leaked:?})"
                ));
            }
        }
    }
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}

/// Pick `entry` from a dropdown by clicking it open and typing to filter,
/// as a player does.
fn pick(u: &mut Ui, screen: ScreenId, popup: &str, entry: &str) -> Result<(), String> {
    try_click(u, screen, popup)?;
    type_text(u, entry);
    press(u, Key::Return, Modifiers::NONE);
    let v = view(u, screen);
    let n = v.id(popup).ok_or_else(|| format!("{popup} is gone"))?;
    match v.selected_text(n) {
        Some(t) if t == entry => Ok(()),
        other => Err(format!("{popup} shows {other:?} after picking {entry:?}")),
    }
}

/// A new events row built only by clicks sends what was picked, and the
/// row it was built from stays as it was.
#[test]
fn an_events_row_built_by_clicks() {
    let Some(pack) = pack() else { return };
    let mut u = new_ui(&pack);
    open_events(&mut u);
    u.drain_actions();
    let screen = ScreenId::WrenchEvents;
    for (popup, entry) in [
        ("WrenchEvent_1_input", "onPlayerTouch"),
        ("WrenchEvent_1_target", "Player"),
        ("WrenchEvent_1_output", "Kill"),
    ] {
        pick(&mut u, screen, popup, entry).unwrap();
    }
    let delay = view(&u, screen).id("WrenchEvent_1_delay").unwrap();
    replace_text(&mut u, screen, delay, "250").unwrap();
    click(&mut u, screen, "wrenchEventsDlg.send();");
    let sent = snapshot(&mut u);
    let rows = &sent["actions"]["SendEvents"]["rows"];
    assert_eq!(
        rows[0]["Editable"],
        serde_json::json!({"enabled": true, "delay_ms": 0, "input": "onActivate", "target": "Self",
            "named_target": null, "output": "setColor", "params": [{"PaintColor": 1}]})
    );
    assert_eq!(
        rows[1]["Editable"],
        serde_json::json!({"enabled": true, "delay_ms": 250, "input": "onPlayerTouch", "target": "Player",
            "named_target": null, "output": "Kill", "params": []}),
        "{sent:#}"
    );
}
