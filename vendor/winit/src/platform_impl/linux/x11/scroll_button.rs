use crate::event::{ElementState, MouseScrollDelta};

// Kept separate so the actual X11 mapping can be exercised without an X server.
pub(crate) fn scroll_button(detail: u32, state: ElementState) -> Option<MouseScrollDelta> {
    // A legacy wheel notch is a button press/release pair. Only the press
    // represents motion; forwarding its release scrolls a second time.
    if state != ElementState::Pressed {
        return None;
    }
    let (x, y) = match detail {
        4 => (0.0, 1.0),
        5 => (0.0, -1.0),
        6 => (1.0, 0.0),
        7 => (-1.0, 0.0),
        _ => return None,
    };
    Some(MouseScrollDelta::LineDelta(x, y))
}
