use std::str::from_utf8;

use serde_json::{json, Value};
use tokio::io::{AsyncWriteExt, BufReader, AsyncReadExt};
use tokio::net::tcp::{WriteHalf, ReadHalf};

use crate::Message;

pub async fn send_message(data: Message, s: &mut WriteHalf<'_>) {
    let data_json = json!({
        "timestamp": data.timestamp,
        "sender": data.sender,
        "style": data.style as u8,
        "text": data.text
    });

    send_json(data_json, s).await;
}

pub async fn send_json(data: Value, s: &mut WriteHalf<'_>) {
    let data_json = data.to_string();

    if data_json.len() > 65535 {
        eprintln!("Data must not exceed 65535 bytes");
        return; 
    }

    let header: [u8; 2] = (data_json.len() as u16).to_be_bytes();
    s.write_all(&[&header, data_json.as_bytes()].concat())
        .await
        .unwrap();
}

pub async fn receive_json(r: &mut BufReader<ReadHalf<'_>>) -> Option<Value> {
    let mut header: [u8; 2] = [0, 0];

    match r.read_exact(&mut header).await {
        Err(_) => return None,
        _ => {},
    };

    let data_size = u16::from_be_bytes(header);
    let mut data_bytes = vec![0_u8; data_size.into()]; 

    match r.read_exact(&mut data_bytes).await {
        Err(_) => return None,
        _ => {},
    };
    
    match from_utf8(&data_bytes) {
        Ok(s) => {
            let res: Result<Value, _> = serde_json::from_str(s);
            match res {
                Ok(v) => Some(v),
                Err(_) => None,
            }
        },
        Err(_) => None,
    }
}