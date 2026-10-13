//! The queue that carries OS callbacks into the event loop.
//!
//! The global hotkey ([`crate::hotkey`]) and the menu-bar menu ([`crate::tray`])
//! report through callbacks the crates invoke on their own schedule (on macOS,
//! on the main thread in the middle of AppKit's event dispatch). Doing real
//! work there — grabbing the screen, reshaping the window — would run inside
//! someone else's stack, so the callbacks only [`push`] a [`NativeEvent`] and
//! wake the loop through `agg_gui::animation::signal_async_state_change`. The
//! shell's idle hook ([`crate::host`]) [`drain`]s the queue on a clean stack.

use std::collections::VecDeque;
use std::sync::{Mutex, MutexGuard};

/// Something the OS asked Screen Snap to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeEvent {
    /// The capture hotkey went down.
    HotkeyPressed,
    /// Menu bar: "Capture Region".
    CaptureFromMenu,
    /// Menu bar: "Settings…".
    ShowSettings,
    /// Menu bar: "Open Captures Folder".
    OpenCapturesFolder,
    /// Menu bar: "Quit Screen Snap".
    Quit,
}

static QUEUE: Mutex<VecDeque<NativeEvent>> = Mutex::new(VecDeque::new());

/// The queue, even if a panicking thread poisoned the lock (the deque itself
/// is always consistent: every critical section is a single push or drain).
fn queue() -> MutexGuard<'static, VecDeque<NativeEvent>> {
    QUEUE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Queue `event` and wake the event loop. Safe from any thread.
pub fn push(event: NativeEvent) {
    queue().push_back(event);
    agg_gui::animation::signal_async_state_change();
}

/// Everything queued so far, oldest first.
pub fn drain() -> Vec<NativeEvent> {
    queue().drain(..).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_come_out_in_order_once() {
        // The queue is process-wide; this is the only test that touches it.
        drain();
        push(NativeEvent::HotkeyPressed);
        push(NativeEvent::Quit);
        let from_thread = std::thread::spawn(|| push(NativeEvent::ShowSettings));
        from_thread.join().expect("pusher thread");
        assert_eq!(
            drain(),
            vec![
                NativeEvent::HotkeyPressed,
                NativeEvent::Quit,
                NativeEvent::ShowSettings
            ]
        );
        assert!(drain().is_empty());
    }
}
