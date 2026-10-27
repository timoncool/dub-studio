//! Server-sent events as the server writes them: `data: <json>` lines, an empty line after each event, and
//! `data: [DONE]` at the end.

use std::io::BufRead;

/// One event of the stream.
#[derive(Debug, PartialEq)]
pub(crate) enum Event {
    Data(String),
    Done,
}

/// Reads the next event; `None` when the stream ended without `[DONE]`.
pub(crate) fn next_event(reader: &mut impl BufRead) -> std::io::Result<Option<Event>> {
    let mut data = String::new();
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            return Ok(if data.is_empty() { None } else { Some(Event::Data(data)) });
        }
        let l = line.trim_end_matches(['\r', '\n']);
        if l.is_empty() {
            if data.is_empty() {
                continue;
            }
            return Ok(Some(if data == "[DONE]" { Event::Done } else { Event::Data(data) }));
        }
        if let Some(rest) = l.strip_prefix("data:") {
            if !data.is_empty() {
                data.push('\n');
            }
            data.push_str(rest.strip_prefix(' ').unwrap_or(rest));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_are_split_on_empty_lines_and_done_ends_them() {
        let text = ": comment\r\ndata: {\"type\":\"a\"}\r\n\r\ndata: {\"type\":\"b\",\ndata: \"x\":1}\n\ndata: [DONE]\n\n";
        let mut r = std::io::BufReader::new(text.as_bytes());
        assert_eq!(next_event(&mut r).unwrap(), Some(Event::Data("{\"type\":\"a\"}".into())));
        assert_eq!(next_event(&mut r).unwrap(), Some(Event::Data("{\"type\":\"b\",\n\"x\":1}".into())));
        assert_eq!(next_event(&mut r).unwrap(), Some(Event::Done));
        assert_eq!(next_event(&mut r).unwrap(), None);
    }
}
