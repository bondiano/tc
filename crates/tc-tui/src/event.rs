use std::time::Duration;

use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};

use crate::error::TuiResult;

/// High-level UI message produced by the event loop.
#[derive(Debug, Clone)]
pub enum Message {
    Tick,
    Quit,
    Key(KeyCode, KeyModifiers),
    Paste(String),
    /// Left-button mouse click at (column, row).
    Click {
        col: u16,
        row: u16,
    },
}

/// Poll crossterm for an event with a timeout, returning a Message if any.
pub fn poll(timeout: Duration) -> TuiResult<Option<Message>> {
    if !event::poll(timeout)? {
        return Ok(None);
    }
    let ev = event::read()?;
    Ok(translate(ev))
}

fn translate(ev: Event) -> Option<Message> {
    match ev {
        Event::Key(KeyEvent {
            code,
            modifiers,
            kind,
            ..
        }) => {
            if kind != KeyEventKind::Press {
                return None;
            }
            Some(Message::Key(code, modifiers))
        }
        Event::Paste(text) => Some(Message::Paste(text)),
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            ..
        }) => Some(Message::Click { col: column, row }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translate_press_yields_key() {
        let ev = Event::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
        let msg = translate(ev);
        assert!(matches!(msg, Some(Message::Key(KeyCode::Char('q'), _))));
    }

    #[test]
    fn translate_paste_yields_paste_message() {
        let msg = translate(Event::Paste("hello".into()));
        assert!(matches!(msg, Some(Message::Paste(ref s)) if s == "hello"));
    }

    #[test]
    fn translate_multiline_paste_preserves_newlines() {
        let msg = translate(Event::Paste("a\nb".into()));
        let Some(Message::Paste(s)) = msg else {
            panic!("expected paste message");
        };
        assert_eq!(s, "a\nb");
    }

    #[test]
    fn translate_key_release_is_ignored() {
        let ev = Event::Key(KeyEvent {
            code: KeyCode::Char('a'),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Release,
            state: crossterm::event::KeyEventState::empty(),
        });
        assert!(translate(ev).is_none());
    }
}
