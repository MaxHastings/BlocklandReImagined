//! Main-menu photographs: stock art and the player's own screenshots. File
//! discovery and decoding run off the frame thread; only the current and next
//! photograph occupy CPU/GPU memory, regardless of the library's size.
use crate::save_picture::Picture;
use bri_ui::{api::MenuBackground, pack::Pack};
use std::{
    path::{Path, PathBuf},
    sync::mpsc,
    time::Duration,
};

const KEY_BASE: u64 = 0x4d45_4e55_0000;
const PERIOD: Duration = Duration::from_secs(5);
const HOLD: Duration = Duration::from_secs(4);

struct Slide {
    path: PathBuf,
    picture: Picture,
    dirty: bool,
}
type Reading = (
    PathBuf,
    usize,
    mpsc::Receiver<anyhow::Result<Option<Picture>>>,
);

pub(crate) struct Backgrounds {
    stock: Vec<PathBuf>,
    directory: PathBuf,
    files: Vec<PathBuf>,
    bag: Vec<PathBuf>,
    scan: Option<mpsc::Receiver<Vec<PathBuf>>>,
    reading: Option<Reading>,
    slides: [Option<Slide>; 2],
    current: Option<usize>,
    next: Option<usize>,
    elapsed: Duration,
    was_open: bool,
    random: u64,
}

impl Backgrounds {
    #[cfg(test)]
    pub(super) fn retain_current_for_test(&mut self) {
        self.stock = vec![
            self.slides[self.current.unwrap()]
                .as_ref()
                .unwrap()
                .path
                .clone(),
        ];
        self.files = self.stock.clone();
        self.bag.clear();
        self.next = None;
        self.reading = None;
        self.elapsed = Duration::ZERO;
    }
    pub fn new(pack: &Pack, state: &Path) -> Self {
        Self {
            stock: pack
                .data
                .images
                .iter()
                .filter(|(key, _)| key.to_ascii_lowercase().starts_with("screenshots/"))
                .map(|(_, image)| pack.dir.join(&image.file))
                .collect(),
            directory: state.join("screenshots"),
            files: Vec::new(),
            bag: Vec::new(),
            scan: None,
            reading: None,
            slides: [None, None],
            current: None,
            next: None,
            elapsed: Duration::ZERO,
            was_open: false,
            random: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(1, |d| d.as_nanos() as u64)
                .max(1),
        }
    }

    fn pick(&mut self) -> Option<PathBuf> {
        let current = self
            .current
            .and_then(|i| self.slides[i].as_ref())
            .map(|s| &s.path);
        if self.bag.is_empty() {
            self.bag = self.files.clone();
            // Fisher-Yates; the seed varies by run, while tests can pin it.
            for i in (1..self.bag.len()).rev() {
                self.random ^= self.random << 13;
                self.random ^= self.random >> 7;
                self.random ^= self.random << 17;
                self.bag.swap(i, (self.random % (i + 1) as u64) as usize);
            }
        }
        // Also check after a failed decode: it may have exposed the current
        // photograph at the top of an already-started shuffle.
        if self.bag.last() == current {
            if self.bag.len() <= 1 {
                return None;
            }
            let last = self.bag.len() - 1;
            self.bag.swap(0, last);
        }
        self.bag.pop()
    }

    pub fn tick(&mut self, dt: Duration, open: bool, runtime: &tokio::runtime::Runtime) {
        if open && !self.was_open && self.scan.is_none() {
            let stock = self.stock.clone();
            let directory = self.directory.clone();
            let (tx, rx) = mpsc::channel();
            runtime.spawn_blocking(move || {
                let mut files = stock;
                if let Ok(entries) = std::fs::read_dir(directory) {
                    files.extend(
                        entries
                            .flatten()
                            .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
                            .map(|e| e.path())
                            .filter(|p| {
                                p.extension().and_then(|s| s.to_str()).is_some_and(|s| {
                                    matches!(
                                        s.to_ascii_lowercase().as_str(),
                                        "png" | "jpg" | "jpeg"
                                    )
                                })
                            }),
                    );
                }
                files.sort();
                files.dedup();
                let _ = tx.send(files);
            });
            self.scan = Some(rx);
        }
        self.was_open = open;
        if !open {
            // A scan started before gameplay cannot describe captures made
            // during that game. Discard it so return always starts a fresh scan.
            self.scan = None;
            return;
        }
        if let Some(rx) = &self.scan {
            match rx.try_recv() {
                Ok(files) => {
                    self.files = files;
                    self.bag.clear();
                    self.scan = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => self.scan = None,
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        // Hold the existing photograph if disk preparation is late. A ready
        // photograph always receives the whole fade, never an abrupt jump.
        if self.current.is_some() {
            self.elapsed = self.elapsed.saturating_add(dt.min(PERIOD));
            if self.next.is_none() {
                self.elapsed = self.elapsed.min(HOLD);
            }
        }
        if let Some((path, slot, rx)) = &self.reading {
            let result = match rx.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some(Err(anyhow::anyhow!("photo reader stopped")))
                }
                Err(mpsc::TryRecvError::Empty) => None,
            };
            if let Some(result) = result {
                let (path, slot) = (path.clone(), *slot);
                match result {
                    Ok(Some(picture)) => {
                        self.slides[slot] = Some(Slide {
                            path,
                            picture,
                            dirty: true,
                        });
                        if self.current.is_none() {
                            self.current = Some(slot);
                        } else {
                            self.next = Some(slot);
                        }
                    }
                    result => {
                        bri_console::warn(format!(
                            "Menu photograph {}: {result:?}",
                            path.display()
                        ));
                        self.files.retain(|p| p != &path);
                    }
                }
                self.reading = None;
            }
        }
        if self.elapsed >= PERIOD && self.next.is_some() {
            self.current = self.next.take();
            self.elapsed = Duration::ZERO;
        }
        if self.reading.is_none()
            && self.next.is_none()
            && let Some(path) = self.pick()
        {
            let slot = self.current.map_or(0, |i| 1 - i);
            let (tx, rx) = mpsc::channel();
            let source = path.clone();
            runtime.spawn_blocking(move || {
                let _ = tx.send(read(&source));
            });
            self.reading = Some((path, slot, rx));
        }
    }

    pub fn view(&self) -> Option<MenuBackground> {
        let picture = |slot: usize| {
            let p = &self.slides[slot].as_ref()?.picture;
            Some(bri_ui::api::PanelPicture {
                key: KEY_BASE + slot as u64,
                size: [p.width, p.height],
            })
        };
        Some(MenuBackground {
            current: picture(self.current?)?,
            next: self.next.and_then(picture),
            blend: self.elapsed.saturating_sub(HOLD).as_secs_f32().min(1.0),
        })
    }

    pub fn gpu_stopped(&mut self) {
        for slide in self.slides.iter_mut().flatten() {
            slide.dirty = true;
        }
    }

    pub fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        renderer: &mut bri_ui::gpu::UiRenderer,
    ) {
        for (i, slide) in self.slides.iter_mut().enumerate() {
            if let Some(slide) = slide
                && slide.dirty
            {
                crate::save_picture::upload_to(
                    device,
                    queue,
                    renderer,
                    KEY_BASE + i as u64,
                    &slide.picture,
                );
                slide.dirty = false;
            }
        }
    }
}

fn read(path: &Path) -> anyhow::Result<Option<Picture>> {
    let Some(image) = crate::save_picture::read_image(path)? else {
        return Ok(None);
    };
    let image = if image.width() > 1920 || image.height() > 1080 {
        image.resize(1920, 1080, image::imageops::FilterType::Triangle)
    } else {
        image
    };
    let rgba = image.into_rgba8();
    Ok(Some(Picture {
        width: rgba.width(),
        height: rgba.height(),
        rgba: rgba.into_raw(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pump(backgrounds: &mut Backgrounds, runtime: &tokio::runtime::Runtime) {
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        loop {
            backgrounds.tick(Duration::ZERO, true, runtime);
            if backgrounds.scan.is_none() && backgrounds.reading.is_none() {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "photo worker did not finish"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn personal_photos_refresh_skip_bad_files_pause_and_keep_only_two_slides() -> anyhow::Result<()>
    {
        let state = tempfile::tempdir()?;
        let directory = state.path().join("screenshots");
        std::fs::create_dir(&directory)?;
        let pack = bri_ui::testing::screens_pack();
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .build()?;
        let mut backgrounds = Backgrounds::new(&pack, state.path());
        pump(&mut backgrounds, &runtime);
        assert!(backgrounds.view().is_none());
        std::fs::write(
            directory.join("one.PNG"),
            crate::testing::png(30, 20, |_, _| [240, 30, 20, 255])?,
        )?;
        std::fs::write(directory.join("bad.jpg"), b"not an image")?;
        backgrounds.tick(Duration::ZERO, false, &runtime);
        pump(&mut backgrounds, &runtime);
        assert!(backgrounds.view().is_some());
        assert!(backgrounds.next.is_none());
        assert_eq!(backgrounds.files.len(), 1);
        let first = backgrounds.slides[backgrounds.current.unwrap()]
            .as_ref()
            .unwrap()
            .path
            .clone();
        // A lone photograph is stable, including while the player is in a game.
        backgrounds.tick(Duration::from_secs(60), false, &runtime);
        assert_eq!(backgrounds.elapsed, Duration::ZERO);
        backgrounds.tick(Duration::ZERO, true, &runtime);
        pump(&mut backgrounds, &runtime);
        let image = image::RgbImage::from_pixel(25, 40, image::Rgb([20, 30, 240]));
        image.save(directory.join("two.jpeg"))?;
        std::fs::create_dir(directory.join("not-a-photo.png"))?;
        backgrounds.random = 1; // Corrupt candidate first, then the current photograph.
        let (_old_scan_sender, old_scan) = mpsc::channel();
        backgrounds.scan = Some(old_scan); // A pre-game scan is still pending.
        backgrounds.tick(Duration::ZERO, false, &runtime);
        pump(&mut backgrounds, &runtime);
        // The scan indexes candidates; corrupt files are removed when decoded.
        let second = backgrounds.slides[backgrounds.next.unwrap()]
            .as_ref()
            .unwrap()
            .path
            .clone();
        assert_ne!(first, second);
        backgrounds.tick(HOLD, true, &runtime);
        assert_eq!(backgrounds.view().unwrap().blend, 0.0);
        backgrounds.tick(Duration::from_millis(500), true, &runtime);
        assert_eq!(backgrounds.view().unwrap().blend, 0.5);
        backgrounds.tick(Duration::from_secs(60), false, &runtime);
        assert_eq!(backgrounds.view().unwrap().blend, 0.5);
        backgrounds.tick(Duration::from_millis(500), true, &runtime);
        assert_eq!(
            backgrounds.slides[backgrounds.current.unwrap()]
                .as_ref()
                .unwrap()
                .path,
            second
        );
        assert_eq!(backgrounds.view().unwrap().blend, 0.0);
        for _ in 0..10 {
            pump(&mut backgrounds, &runtime);
            let before = backgrounds.current;
            backgrounds.tick(PERIOD, true, &runtime);
            assert_ne!(backgrounds.current, before);
            assert_eq!(backgrounds.slides.iter().flatten().count(), 2);
        }
        assert_eq!(backgrounds.files.len(), 2);
        backgrounds.gpu_stopped();
        assert!(backgrounds.slides.iter().flatten().all(|s| s.dirty));
        Ok(())
    }

    #[test]
    fn shuffled_cycles_cover_every_photo_without_adjacent_repeats() {
        let pack = bri_ui::testing::screens_pack();
        let mut backgrounds = Backgrounds::new(&pack, Path::new("unused"));
        backgrounds.files = (0..10).map(|i| PathBuf::from(format!("{i}.png"))).collect();
        backgrounds.random = 42;
        let mut previous = None;
        let mut orders = Vec::new();
        for _ in 0..4 {
            let mut cycle = Vec::new();
            for _ in 0..10 {
                let path = backgrounds.pick().unwrap();
                assert_ne!(previous.as_ref(), Some(&path));
                backgrounds.slides[0] = Some(Slide {
                    path: path.clone(),
                    picture: Picture {
                        width: 1,
                        height: 1,
                        rgba: vec![0; 4],
                    },
                    dirty: true,
                });
                backgrounds.current = Some(0);
                previous = Some(path.clone());
                cycle.push(path);
            }
            let unique: std::collections::BTreeSet<_> = cycle.iter().collect();
            assert_eq!(unique.len(), 10);
            orders.push(cycle);
        }
        assert!(orders.windows(2).any(|orders| orders[0] != orders[1]));
    }
}
