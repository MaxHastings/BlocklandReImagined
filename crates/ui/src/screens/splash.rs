//! An Add-On's splash over the main menu (`splash.json`): pictures fading
//! in over a 640x480 screen scaled to the window, pictures falling from the
//! top, a line after a while, and a click anywhere (once it may close)
//! fading it all out. Slayer's Happy Holidays (`HolidayGreetings.gui`,
//! `holidays_start`, `holidays_snow`, `holidays_stop`).
use super::*;
use crate::api::SplashView;
use crate::draw::Filter;
use crate::pack::TexKey;
use crate::view::EventKind;

const CLOSE: &str = "Splash_Close";
const TIP: &str = "Splash_Tip";

struct Flake {
    texture: u64,
    /// On the 640x480 screen.
    x: f32,
    y: f32,
    speed: f32,
}

pub struct Splash {
    view: View,
    splash: Option<SplashView>,
    /// Since it opened.
    age_ms: u64,
    /// Since the click that closes it.
    closing_ms: Option<u64>,
    flakes: Vec<Flake>,
    /// Time not yet stepped by the falling pictures.
    step_left: u64,
    random: u64,
}

impl Splash {
    pub fn new(core: &Core) -> Self {
        let mut root = ctrl("GuiControl", "GuiDefaultProfile", Rect::new(0, 0, 640, 480));
        root.h_sizing = HSizing::Width;
        root.v_sizing = VSizing::Height;
        // The whole screen takes the click (`HolidayButton`).
        let mut button = ctrl("GuiBitmapButtonCtrl", "GuiDefaultProfile", Rect::new(0, 0, 640, 480));
        button.h_sizing = HSizing::Width;
        button.v_sizing = VSizing::Height;
        button.command = Some(CLOSE.into());
        button.name = Some(CLOSE.into());
        button.text = Some(String::new());
        let splash = core.splash.clone();
        if let Some((text, [x, y, w, h], _)) = splash.as_ref().and_then(|s| s.tip.clone()) {
            let mut tip = ctrl("GuiMLTextCtrl", "GuiMLTextProfile", Rect::new(x, y, w, h));
            tip.text = Some(text);
            tip.name = Some(TIP.into());
            tip.h_sizing = HSizing::Center;
            tip.v_sizing = VSizing::Top;
            root.children.push(tip);
        }
        root.children.push(button);
        let mut view = View::new(&root);
        view.measure(&core.pack);
        if let Some(n) = view.id(TIP) {
            view.set_visible(n, false);
        }
        Self {
            view,
            splash,
            age_ms: 0,
            closing_ms: None,
            flakes: Vec::new(),
            step_left: 0,
            random: core.time_ms.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407),
        }
    }

    fn roll(&mut self, n: u32) -> u32 {
        self.random = self
            .random
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.random >> 33) % u64::from(n.max(1))) as u32
    }

    fn may_close(&self) -> bool {
        self.splash
            .as_ref()
            .is_some_and(|s| self.age_ms >= u64::from(s.close_after_ms))
            && self.closing_ms.is_none()
    }

    /// `holidays_stop`: fade out, the falling pictures speeding up.
    fn close(&mut self) {
        let Some(s) = self.splash.as_ref() else { return };
        let faster = s.falling.as_ref().map_or(0, |f| f.closing_speed) as f32;
        for flake in &mut self.flakes {
            flake.speed += faster;
        }
        self.closing_ms = Some(0);
        if let Some(n) = self.view.id(TIP) {
            self.view.set_visible(n, false);
        }
    }

    /// One `holidays_snow` step: move each picture down, maybe start one.
    fn step(&mut self) {
        let Some(falling) = self.splash.as_ref().and_then(|s| s.falling.clone()) else {
            return;
        };
        self.flakes.retain_mut(|f| {
            f.y += f.speed;
            f.y < 480.0
        });
        if self.closing_ms.is_none() && self.roll(falling.chance) == 0 && !falling.textures.is_empty() {
            let texture = falling.textures[self.roll(falling.textures.len() as u32) as usize];
            let x = self.roll(641) as f32;
            let speed = (falling.speed[0] + self.roll(falling.speed[1] - falling.speed[0] + 1)) as f32;
            self.flakes.push(Flake {
                texture,
                x,
                y: -(falling.size as f32),
                speed,
            });
        }
    }

    /// How opaque everything is now, with a layer's own fade-in.
    fn alpha(&self, fade_in_ms: u32) -> f32 {
        let fade_in = if fade_in_ms == 0 {
            1.0
        } else {
            (self.age_ms as f32 / fade_in_ms as f32).min(1.0)
        };
        let out = match (self.closing_ms, self.splash.as_ref()) {
            (Some(t), Some(s)) if s.fade_out_ms > 0 => 1.0 - (t as f32 / s.fade_out_ms as f32).min(1.0),
            (Some(_), _) => 0.0,
            (None, _) => 1.0,
        };
        fade_in * out
    }
}

impl Screen for Splash {
    fn id(&self) -> ScreenId {
        ScreenId::Splash
    }
    fn view(&self) -> &View {
        &self.view
    }
    fn view_mut(&mut self) -> &mut View {
        &mut self.view
    }
    fn on_event(&mut self, ev: &ViewEvent, _core: &mut Core) {
        if ev.kind == EventKind::Click && command_of(&self.view, ev.node) == CLOSE && self.may_close() {
            self.close();
        }
    }
    fn on_key(&mut self, key: Key, _: Modifiers, _core: &mut Core) -> bool {
        if matches!(key, Key::Escape | Key::Return | Key::Space) && self.may_close() {
            self.close();
        }
        true
    }
    fn tick(&mut self, dt_ms: u64, core: &mut Core) {
        let Some(s) = self.splash.clone() else {
            core.pop(ScreenId::Splash);
            return;
        };
        self.age_ms += dt_ms;
        if let Some((_, _, after)) = &s.tip
            && self.closing_ms.is_none()
            && self.age_ms >= u64::from(*after)
            && let Some(n) = self.view.id(TIP)
        {
            self.view.set_visible(n, true);
        }
        if let Some(t) = self.closing_ms.as_mut() {
            *t += dt_ms;
        }
        if let Some(f) = &s.falling {
            self.step_left += dt_ms;
            let step = u64::from(f.step_ms.max(16));
            while self.step_left >= step {
                self.step_left -= step;
                self.step();
            }
        }
        // Gone once faded and every picture has fallen out.
        if self.closing_ms.is_some_and(|t| t >= u64::from(s.fade_out_ms)) && self.flakes.is_empty() {
            core.splash = None;
            core.pop(ScreenId::Splash);
        }
    }
    fn draw(&self, pack: &Pack, dl: &mut DrawList, core: &Core) {
        let Some(s) = self.splash.as_ref() else { return };
        let (w, h) = core.logical;
        let (sx, sy) = (w as f32 / 640.0, h as f32 / 480.0);
        let tint = |a: f32| [255, 255, 255, (a.clamp(0.0, 1.0) * 255.0) as u8];
        for (texture, rect, fade_in) in &s.layers {
            let [x, y, rw, rh] = rect.unwrap_or([0, 0, 640, 480]);
            dl.image(
                TexKey::External(*texture),
                [0.0, 0.0, 1.0, 1.0],
                [x as f32 * sx, y as f32 * sy, rw as f32 * sx, rh as f32 * sy],
                tint(self.alpha(*fade_in)),
                Filter::Linear,
            );
        }
        if let Some(f) = &s.falling {
            let size = f.size as f32;
            for flake in &self.flakes {
                dl.image(
                    TexKey::External(flake.texture),
                    [0.0, 0.0, 1.0, 1.0],
                    [flake.x * sx, flake.y * sy, size * sx, size * sy],
                    [255; 4],
                    Filter::Linear,
                );
            }
        }
        self.view.draw(pack, dl);
    }
}
