use serde_json::Value;
use std::{
    io::{Read, Write},
    net::TcpStream,
    str::from_utf8,
};

pub fn send_json(data: Value, s: &mut TcpStream) {
    let data_json = data.to_string();

    if data_json.len() > 65535 {
        eprintln!("Data must not exceed 65535 bytes");
        return;
    }

    let header: [u8; 2] = (data_json.len() as u16).to_be_bytes();
    s.write_all(&[&header, data_json.as_bytes()].concat())
        .unwrap();
}

pub fn receive_json(r: &mut TcpStream) -> Option<Value> {
    let mut header: [u8; 2] = [0, 0];

    if r.read_exact(&mut header).is_err() {
        return None;
    }

    let data_size = u16::from_be_bytes(header);
    let mut data_bytes = vec![0_u8; data_size.into()];

    if r.read_exact(&mut data_bytes).is_err() {
        return None;
    }

    match from_utf8(&data_bytes) {
        Ok(s) => {
            let res: Result<Value, _> = serde_json::from_str(s);
            match res {
                Ok(v) => Some(v),
                Err(_) => None,
            }
        }
        Err(_) => None,
    }
}
