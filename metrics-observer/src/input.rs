use std::io;
use std::thread;
use std::time::Duration;

use crossbeam_channel::{bounded, Receiver, RecvTimeoutError, TrySendError};
use termion::event::Key;
use termion::input::TermRead;

pub struct InputEvents {
    rx: Receiver<Key>,
}

impl InputEvents {
    pub fn new() -> InputEvents {
        let (tx, rx) = bounded(1);
        thread::spawn(move || {
            let stdin = io::stdin();
            for key in stdin.keys().flatten() {
                // If our queue is full, we don't care.  The user can just press the key again.
                if let Err(TrySendError::Disconnected(_)) = tx.try_send(key) {
                    eprintln!("input event channel disconnected");
                    return;
                }
            }
        });

        InputEvents { rx }
    }

    pub fn next(&mut self) -> Result<Option<Key>, RecvTimeoutError> {
        match self.rx.recv_timeout(Duration::from_secs(1)) {
            Ok(key) => Ok(Some(key)),
            Err(RecvTimeoutError::Timeout) => Ok(None),
            Err(e) => Err(e),
        }
    }
}
