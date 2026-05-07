//! GPIO button input via Linux sysfs.
//!
//! Reads the four hardware buttons on the AI in a Box (Radxa Rock 5A):
//!
//! | Button | GPIO | Function |
//! |--------|------|----------|
//! | UP     | 63   | Scroll up |
//! | DOWN   | 43   | Scroll down |
//! | ENTER  | 139  | Start / confirm |
//! | BACK   | 138  | Quit / cancel |
//!
//! Buttons are active-low: GPIO reads `0` when pressed, `1` when released.
//! Debouncing requires the button to be held for at least [`DEBOUNCE`] and
//! then released before a press is registered.

use std::fs;
use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use log::{debug, info, warn};

/// Minimum hold time before a press registers.
const DEBOUNCE: Duration = Duration::from_millis(100);

/// Poll interval for GPIO reads.
const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// GPIO pin numbers for each button.
const GPIO_UP: u32 = 63;
const GPIO_DOWN: u32 = 43;
const GPIO_ENTER: u32 = 139;
const GPIO_BACK: u32 = 138;

/// A button press event from the hardware.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonEvent {
    /// UP button (GPIO 63) — scroll up.
    Up,
    /// DOWN button (GPIO 43) — scroll down.
    Down,
    /// ENTER button (GPIO 139) — start/confirm.
    Enter,
    /// BACK button (GPIO 138) — quit/cancel.
    Back,
}

/// State tracker for a single GPIO pin with debouncing.
struct PinState {
    gpio: u32,
    event: ButtonEvent,
    pressed_at: Option<Instant>,
}

impl PinState {
    fn new(gpio: u32, event: ButtonEvent) -> Self {
        Self {
            gpio,
            event,
            pressed_at: None,
        }
    }

    /// Read the current GPIO value. Returns `true` if pressed (active-low).
    fn is_pressed(&self) -> bool {
        let path = format!("/sys/class/gpio/gpio{}/value", self.gpio);
        fs::read_to_string(&path)
            .map(|v| v.trim() == "0")
            .unwrap_or(false)
    }

    /// Update state and return a button event if a debounced press-and-release occurred.
    fn poll(&mut self) -> Option<ButtonEvent> {
        let pressed = self.is_pressed();

        match (pressed, self.pressed_at) {
            // Button just pressed — start timing
            (true, None) => {
                self.pressed_at = Some(Instant::now());
                None
            }
            // Button released after debounce threshold — fire event
            (false, Some(at)) if at.elapsed() >= DEBOUNCE => {
                self.pressed_at = None;
                Some(self.event)
            }
            // Button released too quickly — ignore
            (false, Some(_)) => {
                self.pressed_at = None;
                None
            }
            // Still held or still released — no change
            _ => None,
        }
    }
}

/// Export a GPIO pin via sysfs if not already exported.
fn export_gpio(gpio: u32) -> bool {
    let gpio_path = format!("/sys/class/gpio/gpio{gpio}");
    if Path::new(&gpio_path).exists() {
        return true;
    }

    if fs::write("/sys/class/gpio/export", gpio.to_string()).is_err() {
        warn!("failed to export GPIO {gpio} (need root?)");
        return false;
    }

    // Wait for sysfs to create the directory
    thread::sleep(Duration::from_millis(50));

    // Set direction to input
    let dir_path = format!("{gpio_path}/direction");
    if fs::write(&dir_path, "in").is_err() {
        warn!("failed to set GPIO {gpio} direction to input");
        return false;
    }

    true
}

/// Start a background thread that polls GPIO buttons and sends events.
///
/// Returns `None` if GPIO is not available (e.g. running on `x86_64` dev machine).
/// The TUI should handle this gracefully — buttons are optional.
pub fn start_polling() -> Option<mpsc::Receiver<ButtonEvent>> {
    let gpios = [GPIO_UP, GPIO_DOWN, GPIO_ENTER, GPIO_BACK];

    // Check if sysfs GPIO is available at all
    if !Path::new("/sys/class/gpio").exists() {
        info!("GPIO sysfs not available — button input disabled");
        return None;
    }

    // Export all pins
    let mut all_ok = true;
    for &gpio in &gpios {
        if !export_gpio(gpio) {
            all_ok = false;
        }
    }

    if !all_ok {
        warn!("some GPIO pins failed to export — button input may be partial");
    }

    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let mut pins = vec![
            PinState::new(GPIO_UP, ButtonEvent::Up),
            PinState::new(GPIO_DOWN, ButtonEvent::Down),
            PinState::new(GPIO_ENTER, ButtonEvent::Enter),
            PinState::new(GPIO_BACK, ButtonEvent::Back),
        ];

        info!("GPIO button polling started");

        loop {
            for pin in &mut pins {
                if let Some(event) = pin.poll() {
                    debug!("button event: {event:?}");
                    if tx.send(event).is_err() {
                        // Receiver dropped — main thread exited
                        info!("GPIO polling stopped (receiver dropped)");
                        return;
                    }
                }
            }
            thread::sleep(POLL_INTERVAL);
        }
    });

    Some(rx)
}
