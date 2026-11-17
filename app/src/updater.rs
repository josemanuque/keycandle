//! Firmware updates as a background job: fetch the image, restart the keyboard into its
//! bootloader, write, start the new firmware. The UI polls `status()`; nothing here blocks it.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};

use crate::device::Keyboard;
use crate::isp::{self, Bootloader};
use crate::releases::{self, Release};

/// Where the new firmware comes from.
#[derive(Clone)]
pub enum Source {
    Release(Release),
    File(PathBuf),
}

impl Source {
    pub fn describe(&self) -> String {
        match self {
            Source::Release(r) => r.tag.clone(),
            Source::File(p) => p.file_name().map_or("the file".into(), |n| n.to_string_lossy().into_owned()),
        }
    }

    fn fetch(&self) -> Result<Vec<u8>> {
        match self {
            Source::Release(r) => r.download(),
            Source::File(p) => std::fs::read(p).with_context(|| format!("cannot read {}", p.display())),
        }
    }
}

#[derive(Clone, PartialEq)]
pub enum Step {
    Idle,
    Downloading,
    /// `manual`: the firmware could not restart itself; the user presses Fn + Pause.
    WaitingForBootloader { manual: bool },
    Writing(f32),
    Starting,
    Done(String),
    Failed(String),
}

impl Step {
    pub fn is_busy(&self) -> bool {
        !matches!(self, Step::Idle | Step::Done(_) | Step::Failed(_))
    }
}

/// The release list: not asked yet, being fetched, or the answer.
#[derive(Clone)]
pub enum Releases {
    NotChecked,
    Checking,
    Loaded(Vec<Release>),
    Failed(String),
}

#[derive(Clone)]
pub struct Updater {
    step: Arc<Mutex<Step>>,
    releases: Arc<Mutex<Releases>>,
}

/// How long to wait for the bootloader after asking the firmware to restart into it, and after
/// asking the user to press Fn + Pause.
const AUTO_WAIT: Duration = Duration::from_secs(8);
const MANUAL_WAIT: Duration = Duration::from_secs(120);
/// How long the new firmware may take to show up on USB.
const START_WAIT: Duration = Duration::from_secs(10);

impl Default for Updater {
    fn default() -> Self {
        Self { step: Arc::new(Mutex::new(Step::Idle)), releases: Arc::new(Mutex::new(Releases::NotChecked)) }
    }
}

impl Updater {
    pub fn step(&self) -> Step {
        self.step.lock().unwrap().clone()
    }

    pub fn releases(&self) -> Releases {
        self.releases.lock().unwrap().clone()
    }

    /// Fetch the release list from GitHub in the background.
    pub fn check(&self) {
        *self.releases.lock().unwrap() = Releases::Checking;
        let releases = self.releases.clone();
        thread::spawn(move || {
            let result = match releases::fetch() {
                Ok(list) => Releases::Loaded(list),
                Err(e) => Releases::Failed(format!("{e:#}")),
            };
            *releases.lock().unwrap() = result;
        });
    }

    /// Install firmware. `keyboard` (when connected with this firmware) is asked to restart into
    /// its bootloader; without it the user does that with Fn + Pause.
    pub fn install(&self, source: Source, keyboard: Option<Keyboard>) {
        let step = self.step.clone();
        let set = move |s: Step| *step.lock().unwrap() = s;
        set(Step::Downloading);
        thread::spawn(move || {
            let result = run_install(&source, keyboard, &set);
            set(match result {
                Ok(()) => Step::Done(format!("{} is installed.", source.describe())),
                Err(e) => Step::Failed(format!("{e:#}")),
            });
        });
    }

    /// Leave the bootloader without writing anything (e.g. after Fn + Pause by accident).
    pub fn leave_bootloader(&self) {
        let step = self.step.clone();
        *step.lock().unwrap() = Step::Starting;
        thread::spawn(move || {
            let result = Bootloader::open().and_then(|bl| bl.start_firmware());
            *step.lock().unwrap() = match result {
                Ok(()) => Step::Done("Left flash mode; the keyboard is starting.".into()),
                Err(e) => Step::Failed(format!("{e:#}")),
            };
        });
    }

    /// Forget a finished or failed job.
    pub fn dismiss(&self) {
        let mut step = self.step.lock().unwrap();
        if !step.is_busy() {
            *step = Step::Idle;
        }
    }
}

fn run_install(source: &Source, keyboard: Option<Keyboard>, set: &impl Fn(Step)) -> Result<()> {
    let image = source.fetch()?;
    isp::check_image(&image)?;

    // Ask whatever firmware is running to restart into the bootloader: this one over VIA, the
    // original one with its own command. Older versions of this firmware can't; the user then
    // presses Fn + Pause.
    let from_stock = keyboard.is_none() && !isp::is_present() && isp::stock_present();
    let restarted = match keyboard {
        Some(kb) => kb.enter_bootloader().is_ok(),
        None => from_stock && isp::enter_from_stock().is_ok(),
    };
    if from_stock && restarted {
        // The stock firmware's jump keeps its USB id: no new device appears, the bootloader just
        // answers on the same one (Bootloader::open finds it there).
        thread::sleep(Duration::from_secs(1));
    } else if !isp::is_present() {
        set(Step::WaitingForBootloader { manual: !restarted });
        let found = (restarted && isp::wait_for(AUTO_WAIT)) || {
            set(Step::WaitingForBootloader { manual: true });
            isp::wait_for(MANUAL_WAIT)
        };
        if !found {
            anyhow::bail!("the keyboard never entered flash mode; nothing was written");
        }
    }
    thread::sleep(Duration::from_millis(500)); // let the OS finish setting up the device

    let bootloader = Bootloader::open()?;
    set(Step::Writing(0.0));
    bootloader.write(&image, |p| set(Step::Writing(p)))?;
    set(Step::Starting);
    bootloader.start_firmware()?;
    // Best effort: the new firmware shows up again (this firmware, or stock's Apple-id keyboard).
    let _ = wait_until(START_WAIT, || !isp::is_present());
    Ok(())
}

fn wait_until(timeout: Duration, done: impl Fn() -> bool) -> bool {
    let end = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < end {
        if done() {
            return true;
        }
        thread::sleep(Duration::from_millis(250));
    }
    false
}
