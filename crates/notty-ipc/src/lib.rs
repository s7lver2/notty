pub const PIPE_NAME: &str = r"\\.\pipe\notty-instance";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    OpenPath(String),
    NewTemp,
    NewPermanent,
}

pub fn encode(msg: &Message) -> Vec<u8> {
    let line = match msg {
        Message::OpenPath(p) => format!("OPEN {p}"),
        Message::NewTemp => "NEW_TEMP".to_string(),
        Message::NewPermanent => "NEW_PERMANENT".to_string(),
    };
    let mut out = line.into_bytes();
    out.push(b'\n');
    out
}

pub fn decode(bytes: &[u8]) -> Option<Message> {
    let text = std::str::from_utf8(bytes).ok()?.trim_end_matches('\n');
    if let Some(path) = text.strip_prefix("OPEN ") {
        return Some(Message::OpenPath(path.to_string()));
    }
    match text {
        "NEW_TEMP" => Some(Message::NewTemp),
        "NEW_PERMANENT" => Some(Message::NewPermanent),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_open_path() {
        let msg = Message::OpenPath(r"C:\Users\ana\nota.txt".to_string());
        assert_eq!(decode(&encode(&msg)), Some(msg));
    }

    #[test]
    fn round_trips_new_temp_and_permanent() {
        assert_eq!(decode(&encode(&Message::NewTemp)), Some(Message::NewTemp));
        assert_eq!(decode(&encode(&Message::NewPermanent)), Some(Message::NewPermanent));
    }

    #[test]
    fn encoded_message_ends_with_newline() {
        assert!(encode(&Message::NewTemp).ends_with(b"\n"));
    }

    #[test]
    fn decode_of_garbage_is_none() {
        assert_eq!(decode(b"algo-que-no-es-un-mensaje\n"), None);
    }

    #[test]
    fn decode_of_empty_is_none() {
        assert_eq!(decode(b""), None);
    }
}
