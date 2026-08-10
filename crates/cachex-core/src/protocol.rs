

use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    Ping,
    Set { key: String, value: Vec<u8>, ttl: Option<u64> },
    Get { key: String },
    Delete { key: String },
    Info,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Response {
    Pong,
    Ok,
    Value(Vec<u8>),
    NotFound,
    Error(String),
    Info(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolError {
    Empty,
    UnknownCommand(String),
    WrongArity(String),
    InvalidResponse,
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProtocolError::Empty => write!(f, "empty request"),
            ProtocolError::UnknownCommand(c) => write!(f, "unknown command: {c}"),
            ProtocolError::WrongArity(c) => write!(f, "wrong argument count for {c}"),
            ProtocolError::InvalidResponse => write!(f, "malformed response"),
        }
    }
}

impl std::error::Error for ProtocolError {}

pub fn parse_command(line: &str) -> Result<Command, ProtocolError> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Err(ProtocolError::Empty);
    }

    let mut words = trimmed.split_whitespace();
    let verb = words.next().unwrap_or_default().to_ascii_uppercase();
    let args: Vec<&str> = words.collect();

    match verb.as_str() {
        "PING" => Ok(Command::Ping),
        "INFO" => Ok(Command::Info),
        "GET" => {
            if args.len() != 1 {
                return Err(ProtocolError::WrongArity("GET".to_string()));
            }
            Ok(Command::Get {
                key: args[0].to_string(),
            })
        }
        "DELETE" => {
            if args.len() != 1 {
                return Err(ProtocolError::WrongArity("DELETE".to_string()));
            }
            Ok(Command::Delete {
                key: args[0].to_string(),
            })
        }
        "SET" => {
            if args.len() < 2 {
                return Err(ProtocolError::WrongArity("SET".to_string()));
            }
            let key = args[0].to_string();
            let mut value_end = args.len();
            let mut ttl = None;
            if args.len() > 2 {
                if let Ok(secs) = args[args.len() - 1].parse::<u64>() {
                    ttl = Some(secs);
                    value_end -= 1;
                }
            }
            let value = args[1..value_end].join(" ").into_bytes();
            Ok(Command::Set { key, value, ttl })
        }
        other => Err(ProtocolError::UnknownCommand(other.to_string())),
    }
}

pub fn encode_command(command: &Command) -> String {
    match command {
        Command::Ping => "PING".to_string(),
        Command::Info => "INFO".to_string(),
        Command::Get { key } => format!("GET {key}"),
        Command::Delete { key } => format!("DELETE {key}"),
        Command::Set { key, value, ttl } => {
            let value = String::from_utf8_lossy(value);
            match ttl {
                Some(ttl) => format!("SET {key} {value} {ttl}"),
                None => format!("SET {key} {value}"),
            }
        }
    }
}

pub fn encode_response(response: &Response) -> Vec<u8> {
    let line = match response {
        Response::Pong => "PONG".to_string(),
        Response::Ok => "OK".to_string(),
        Response::Value(value) => format!("VALUE {}", String::from_utf8_lossy(value)),
        Response::NotFound => "NOTFOUND".to_string(),
        Response::Error(message) => format!("ERROR {message}"),
        Response::Info(text) => format!("INFO {text}"),
    };
    format!("{line}\n").into_bytes()
}

pub fn parse_response(line: &str) -> Result<Response, ProtocolError> {
    let line = line.trim_end_matches('\n').trim_end_matches('\r');
    let mut parts = line.splitn(2, char::is_whitespace);
    let tag = parts.next().unwrap_or_default();
    let rest = parts.next().unwrap_or("");
    match tag {
        "PONG" => Ok(Response::Pong),
        "OK" => Ok(Response::Ok),
        "NOTFOUND" => Ok(Response::NotFound),
        "VALUE" => Ok(Response::Value(rest.as_bytes().to_vec())),
        "ERROR" => Ok(Response::Error(rest.to_string())),
        "INFO" => Ok(Response::Info(rest.to_string())),
        _ => Err(ProtocolError::InvalidResponse),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_set_without_ttl() {
        let cmd = parse_command("SET user:101 Jill").unwrap();
        assert_eq!(
            cmd,
            Command::Set {
                key: "user:101".to_string(),
                value: b"Jill".to_vec(),
                ttl: None,
            }
        );
    }

    #[test]
    fn parse_set_with_ttl() {
        let cmd = parse_command("SET session:123 abc123 60").unwrap();
        assert_eq!(
            cmd,
            Command::Set {
                key: "session:123".to_string(),
                value: b"abc123".to_vec(),
                ttl: Some(60),
            }
        );
    }

    #[test]
    fn parse_set_value_with_spaces() {
        let cmd = parse_command("SET note hello world").unwrap();
        assert_eq!(
            cmd,
            Command::Set {
                key: "note".to_string(),
                value: b"hello world".to_vec(),
                ttl: None,
            }
        );
    }

    #[test]
    fn parse_get_delete_ping_info() {
        assert_eq!(parse_command("GET user:101").unwrap(), Command::Get { key: "user:101".into() });
        assert_eq!(parse_command("DELETE user:101").unwrap(), Command::Delete { key: "user:101".into() });
        assert_eq!(parse_command("PING").unwrap(), Command::Ping);
        assert_eq!(parse_command("INFO").unwrap(), Command::Info);
        assert_eq!(parse_command("ping").unwrap(), Command::Ping);
    }

    #[test]
    fn parse_errors() {
        assert_eq!(parse_command(""), Err(ProtocolError::Empty));
        assert!(matches!(parse_command("BOGUS x"), Err(ProtocolError::UnknownCommand(_))));
        assert!(matches!(parse_command("GET a b"), Err(ProtocolError::WrongArity(_))));
        assert!(matches!(parse_command("SET onlykey"), Err(ProtocolError::WrongArity(_))));
    }

    #[test]
    fn command_round_trip() {
        let commands = vec![
            Command::Ping,
            Command::Info,
            Command::Set { key: "k1".into(), value: b"v1".to_vec(), ttl: None },
            Command::Set { key: "k2".into(), value: b"v2".to_vec(), ttl: Some(42) },
            Command::Get { key: "k1".into() },
            Command::Delete { key: "k1".into() },
        ];
        for command in commands {
            let line = encode_command(&command);
            assert_eq!(parse_command(&line).unwrap(), command, "round trip failed for {line}");
        }
    }

    #[test]
    fn response_round_trip() {
        let responses = vec![
            Response::Pong,
            Response::Ok,
            Response::Value(b"Jill".to_vec()),
            Response::NotFound,
            Response::Error("something broke".to_string()),
            Response::Info("keys=0 hits=0".to_string()),
        ];
        for response in responses {
            let line = String::from_utf8(encode_response(&response)).unwrap();
            assert_eq!(parse_response(&line).unwrap(), response, "round trip failed for {line}");
        }
    }
}