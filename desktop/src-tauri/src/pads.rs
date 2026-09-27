//! Binding a control in the builder by pressing it on a controller.
//!
//! We read the press through gilrs, in which the buttons have names for
//! their position (south, east, left trigger), as the RetroArch controller
//! profiles map the pad of every player to the standard pad. So we record a
//! position that is valid on any pad, never the button number of one pad
//! model. gilrs works on both macOS and Windows.

use gilrs::{Button, EventType, Gilrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

static CANCELLED: AtomicBool = AtomicBool::new(false);

/// How long to wait for a controller before we report that there is none.
const FIND_A_CONTROLLER: Duration = Duration::from_secs(1);

/// Stop a capture that is waiting.
pub fn cancel() {
    CANCELLED.store(true, Ordering::SeqCst);
}

/// The pad position of a gilrs button, as named in `controls.json`.
fn position(button: Button) -> Option<&'static str> {
    Some(match button {
        Button::South => "b",
        Button::East => "a",
        Button::West => "y",
        Button::North => "x",
        Button::LeftTrigger => "l",
        Button::RightTrigger => "r",
        Button::LeftTrigger2 => "l2",
        Button::RightTrigger2 => "r2",
        Button::LeftThumb => "l3",
        Button::RightThumb => "r3",
        Button::Select => "select",
        Button::Start => "start",
        Button::DPadUp => "up",
        Button::DPadDown => "down",
        Button::DPadLeft => "left",
        Button::DPadRight => "right",
        _ => return None,
    })
}

/// Wait up to `wait` for a button on any connected controller, and return
/// the pad position of that button. `None` when nothing was pressed in time
/// or the wait was cancelled.
pub fn capture(wait: Duration) -> Result<Option<String>, String> {
    CANCELLED.store(false, Ordering::SeqCst);
    let mut gilrs =
        Gilrs::new().map_err(|error| format!("Controllers could not be read: {error}"))?;
    let started = Instant::now();
    while started.elapsed() < wait && !CANCELLED.load(Ordering::SeqCst) {
        while let Some(event) = gilrs.next_event() {
            if let EventType::ButtonPressed(button, _) = event.event {
                if let Some(found) = position(button) {
                    return Ok(Some(found.to_string()));
                }
            }
        }
        if started.elapsed() > FIND_A_CONTROLLER && gilrs.gamepads().next().is_none() {
            return Err("No controller is connected.".into());
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The player can press every position that a control can move to, and
    /// every press maps to one of those positions.
    #[test]
    fn every_pad_position_can_be_pressed() {
        let buttons = [
            Button::South,
            Button::East,
            Button::North,
            Button::West,
            Button::C,
            Button::Z,
            Button::LeftTrigger,
            Button::LeftTrigger2,
            Button::RightTrigger,
            Button::RightTrigger2,
            Button::Select,
            Button::Start,
            Button::Mode,
            Button::LeftThumb,
            Button::RightThumb,
            Button::DPadUp,
            Button::DPadDown,
            Button::DPadLeft,
            Button::DPadRight,
            Button::Unknown,
        ];
        let mut pressed: Vec<&str> = buttons.into_iter().filter_map(position).collect();
        pressed.sort();
        let mut declared: Vec<String> = crate::controls::pad_positions()
            .unwrap()
            .into_iter()
            .map(|position| position.id)
            .collect();
        declared.sort();
        assert_eq!(pressed, declared);
    }
}
