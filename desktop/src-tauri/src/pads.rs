//! Binding a control in the builder by pressing it on a controller.
//!
//! We read the press through gilrs, where a controller's buttons and axes are
//! named after their place (south, east, left trigger, left stick), as on the
//! standard pad of RetroArch's controller profiles. So we record a position
//! that is the same on any pad, never the button or axis number of one pad
//! model. With gilrs we read controllers the same way on macOS and Windows,
//! and up is positive on every stick.
//!
//! We read the controllers on one thread for as long as the builder runs. So
//! we know where each axis rests, and we do not take a button or stick still
//! held from the previous press as the new one.

use gilrs::{Axis, Button, EventType, Gilrs};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// The newest capture requested and the newest one cancelled. We stop a
/// capture once someone cancels it or asks for a newer one.
static ASKED: AtomicU64 = AtomicU64::new(0);
static CANCELLED: AtomicU64 = AtomicU64::new(0);

/// How far an axis must be from centre, and from where it rested, to count as
/// pressed. This is the same share of its travel as in RetroArch's own binder
/// (20000 of 32767, `menu_input_key_bind_poll_find_trigger_pad`).
const PRESSED: f32 = 20000.0 / 32767.0;

/// Stop the capture that is waiting.
pub fn cancel() {
    CANCELLED.store(ASKED.load(Ordering::SeqCst), Ordering::SeqCst);
}

fn stopped(capture: u64) -> bool {
    CANCELLED.load(Ordering::SeqCst) >= capture || ASKED.load(Ordering::SeqCst) > capture
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

/// The pad position for an axis moved in that direction: a stick direction,
/// a d-pad that appears as an axis, or an analogue trigger that appears as
/// one. A trigger counts only when pressed, never in its resting position.
fn direction(axis: Axis, positive: bool) -> Option<&'static str> {
    Some(match (axis, positive) {
        (Axis::LeftStickX, true) => "l_x_plus",
        (Axis::LeftStickX, false) => "l_x_minus",
        (Axis::LeftStickY, true) => "l_y_minus",
        (Axis::LeftStickY, false) => "l_y_plus",
        (Axis::RightStickX, true) => "r_x_plus",
        (Axis::RightStickX, false) => "r_x_minus",
        (Axis::RightStickY, true) => "r_y_minus",
        (Axis::RightStickY, false) => "r_y_plus",
        (Axis::DPadX, true) => "right",
        (Axis::DPadX, false) => "left",
        (Axis::DPadY, true) => "up",
        (Axis::DPadY, false) => "down",
        (Axis::LeftZ, true) => "l2",
        (Axis::RightZ, true) => "r2",
        _ => return None,
    })
}

/// An event from a controller during a capture.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Input {
    Button(Button),
    /// An axis moved, on the controller numbered `pad`, to `value`.
    Axis { pad: usize, axis: Axis, value: f32 },
}

/// The pad position of the first press in a capture. An axis counts once it
/// is far from centre and far from its resting place, which is where it was
/// when the capture began, or the centre once the player lets it go back.
#[derive(Default)]
struct Watch {
    rest: HashMap<(usize, Axis), f32>,
}

impl Watch {
    fn resting(rest: impl IntoIterator<Item = ((usize, Axis), f32)>) -> Self {
        Self {
            rest: rest.into_iter().collect(),
        }
    }

    fn press(&mut self, input: Input) -> Option<&'static str> {
        match input {
            Input::Button(button) => position(button),
            Input::Axis { pad, axis, value } => {
                let rest = self.rest.entry((pad, axis)).or_insert(0.0);
                if value.abs() < PRESSED / 2.0 {
                    *rest = 0.0;
                    return None;
                }
                (value.abs() >= PRESSED && (value - *rest).abs() >= PRESSED)
                    .then(|| direction(axis, value > 0.0))
                    .flatten()
            }
        }
    }
}

fn input(event: EventType, pad: usize) -> Option<Input> {
    match event {
        EventType::ButtonPressed(button, _) => Some(Input::Button(button)),
        EventType::AxisChanged(axis, value, _) => Some(Input::Axis { pad, axis, value }),
        _ => None,
    }
}

type Answer = Result<Option<String>, String>;

struct Capture {
    number: u64,
    wait: Duration,
    answer: Sender<Answer>,
}

/// The thread on which we read the controllers. We start it at the first
/// capture and keep reading between captures, so the state of each
/// controller is current and no events pile up.
fn controllers() -> Sender<Capture> {
    static CAPTURES: OnceLock<Mutex<Sender<Capture>>> = OnceLock::new();
    CAPTURES
        .get_or_init(|| {
            let (captures, asked) = channel::<Capture>();
            std::thread::spawn(move || {
                let mut gilrs = Gilrs::new().map_err(|error| error.to_string());
                loop {
                    let capture = match asked.recv_timeout(Duration::from_millis(100)) {
                        Ok(capture) => capture,
                        Err(RecvTimeoutError::Timeout) => {
                            if let Ok(gilrs) = &mut gilrs {
                                while gilrs.next_event().is_some() {}
                            }
                            continue;
                        }
                        Err(RecvTimeoutError::Disconnected) => return,
                    };
                    let answer = match &mut gilrs {
                        Ok(gilrs) => listen(gilrs, capture.number, capture.wait),
                        Err(error) => Err(format!("Controllers could not be read: {error}")),
                    };
                    let _ = capture.answer.send(answer);
                }
            });
            Mutex::new(captures)
        })
        .lock()
        .expect("the controllers' thread is asked one capture at a time")
        .clone()
}

/// Wait up to `wait` for a press on any connected controller, and return its
/// pad position. A button or axis already held when the wait began does not
/// count.
fn listen(gilrs: &mut Gilrs, number: u64, wait: Duration) -> Answer {
    while gilrs.next_event().is_some() {}
    let mut watch = Watch::resting(gilrs.gamepads().flat_map(|(id, pad)| {
        let pad_number = usize::from(id);
        [
            Axis::LeftStickX,
            Axis::LeftStickY,
            Axis::RightStickX,
            Axis::RightStickY,
            Axis::LeftZ,
            Axis::RightZ,
            Axis::DPadX,
            Axis::DPadY,
        ]
        .into_iter()
        .map(move |axis| ((pad_number, axis), pad.value(axis)))
        .collect::<Vec<_>>()
    }));
    let started = Instant::now();
    while started.elapsed() < wait && !stopped(number) {
        while let Some(event) = gilrs.next_event() {
            let pressed = input(event.event, usize::from(event.id)).and_then(|said| watch.press(said));
            if let Some(found) = pressed {
                return Ok(Some(found.to_string()));
            }
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    Ok(None)
}

/// Wait up to `wait` for a press on any connected controller, and return the
/// pad position it is at. Returns `None` when nobody pressed anything in time,
/// or someone cancelled the wait or asked for a newer capture. With no
/// controller connected, we wait the whole time.
pub fn capture(wait: Duration) -> Answer {
    let number = ASKED.fetch_add(1, Ordering::SeqCst) + 1;
    let (answer, answered) = channel();
    controllers()
        .send(Capture { number, wait, answer })
        .map_err(|_| "Controllers could not be read.".to_string())?;
    answered
        .recv()
        .map_err(|_| "Controllers could not be read.".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declared() -> Vec<String> {
        let mut ids: Vec<String> = crate::controls::pad_positions()
            .unwrap()
            .into_iter()
            .map(|position| position.id)
            .collect();
        ids.sort();
        ids
    }

    /// The player can press every position the author can move a control to,
    /// as a button or an axis moved one way, and no press gives another one.
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
        let axes = [
            Axis::LeftStickX,
            Axis::LeftStickY,
            Axis::LeftZ,
            Axis::RightStickX,
            Axis::RightStickY,
            Axis::RightZ,
            Axis::DPadX,
            Axis::DPadY,
            Axis::Unknown,
        ];
        let mut pressed: Vec<String> = buttons
            .into_iter()
            .filter_map(position)
            .chain(
                axes.into_iter()
                    .flat_map(|axis| [direction(axis, true), direction(axis, false)])
                    .flatten(),
            )
            .map(str::to_string)
            .collect();
        pressed.sort();
        pressed.dedup();
        assert_eq!(pressed, declared());
    }

    fn axis(axis: Axis, value: f32) -> Input {
        Input::Axis { pad: 0, axis, value }
    }

    /// A stick moved up gives its up direction, with its RetroArch name. In
    /// libretro up is minus, and in gilrs it is plus.
    #[test]
    fn a_stick_pushed_is_its_direction() {
        let mut watch = Watch::default();
        assert_eq!(watch.press(axis(Axis::LeftStickY, 0.3)), None);
        assert_eq!(watch.press(axis(Axis::LeftStickY, 0.9)), Some("l_y_minus"));
        let mut watch = Watch::default();
        assert_eq!(watch.press(axis(Axis::RightStickX, -1.0)), Some("r_x_minus"));
        let mut watch = Watch::default();
        assert_eq!(watch.press(Input::Button(Button::South)), Some("b"));
    }

    /// A stick still held from the previous press does not count again until
    /// the player lets it go back, and then it counts.
    #[test]
    fn a_held_stick_counts_only_once_let_back() {
        let mut watch = Watch::resting([((0, Axis::LeftStickY), 1.0)]);
        assert_eq!(watch.press(axis(Axis::LeftStickY, 0.95)), None);
        assert_eq!(watch.press(axis(Axis::LeftStickX, 0.8)), Some("l_x_plus"));
        assert_eq!(watch.press(axis(Axis::LeftStickY, 0.1)), None);
        assert_eq!(watch.press(axis(Axis::LeftStickY, 1.0)), Some("l_y_minus"));
    }

    /// On some pads, an analogue trigger that appears as an axis rests at one
    /// end. It counts as pressed away from there, and never at its resting end.
    #[test]
    fn a_trigger_counts_pressed_from_where_it_rests() {
        let mut watch = Watch::resting([((0, Axis::LeftZ), -1.0)]);
        assert_eq!(watch.press(axis(Axis::LeftZ, -1.0)), None);
        assert_eq!(watch.press(axis(Axis::LeftZ, 1.0)), Some("l2"));
        let mut unknown = Watch::default();
        assert_eq!(unknown.press(axis(Axis::RightZ, -1.0)), None);
    }

    /// We track the resting position of each controller's axes separately.
    #[test]
    fn two_controllers_rest_apart() {
        let mut watch = Watch::resting([((0, Axis::LeftStickX), 1.0)]);
        assert_eq!(
            watch.press(Input::Axis { pad: 1, axis: Axis::LeftStickX, value: 1.0 }),
            Some("l_x_plus")
        );
    }
}
