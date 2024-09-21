use std::io;
use std::time::Duration;

use ratatui::crossterm::event::{self, Event, KeyEvent, KeyEventKind};

pub struct InputEvents;

impl InputEvents {
    pub fn next() -> io::Result<Option<KeyEvent>> {
        if event::poll(Duration::from_secs(1))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => return Ok(Some(key)),
                _ => {}
            }
        }
        Ok(None)
    }
}
