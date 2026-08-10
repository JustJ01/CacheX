

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InternalMessage {
    
    Ping,
    
    Upsert {
        key: String,
        value: Vec<u8>,
        ttl: Option<u64>,
    },
    
    Delete { key: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InternalResponse {
    Pong,
    Ok,
    Error(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_round_trip_bincode() {
        let messages = vec![
            InternalMessage::Ping,
            InternalMessage::Upsert {
                key: "user:101".into(),
                value: b"Jill".to_vec(),
                ttl: Some(60),
            },
            InternalMessage::Delete { key: "user:101".into() },
        ];
        for message in messages {
            let bytes = bincode::serialize(&message).unwrap();
            let decoded: InternalMessage = bincode::deserialize(&bytes).unwrap();
            assert_eq!(decoded, message);
        }
    }

    #[test]
    fn responses_round_trip_bincode() {
        let responses = vec![
            InternalResponse::Pong,
            InternalResponse::Ok,
            InternalResponse::Error("boom".into()),
        ];
        for response in responses {
            let bytes = bincode::serialize(&response).unwrap();
            let decoded: InternalResponse = bincode::deserialize(&bytes).unwrap();
            assert_eq!(decoded, response);
        }
    }
}