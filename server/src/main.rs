use chrono::*;
use colored::*;
use futures::lock::Mutex;
use serde_json::*;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::sync::broadcast;
//use openssl::rsa::*; TODO szyfrowanie

type Db = Arc<Mutex<HashMap<String, SocketAddr>>>;

const MESSAGE_MAX_SIZE: usize = 512;

#[tokio::main]
async fn main() {
    let listener = TcpListener::bind("localhost:2115").await.unwrap();

    let db: Db = Arc::new(Mutex::new(HashMap::new()));

    let (tx, _rx) = broadcast::channel(10);

    loop {
        let (mut socket, addr) = listener.accept().await.unwrap();

        let db = db.clone();
        let tx = tx.clone();
        let mut rx = tx.subscribe();

        tokio::spawn(async move {
            let (reader, mut writer) = socket.split();

            let mut reader = BufReader::new(reader);
            let mut line = String::new();
            let mut username: String = String::new();

            // Dodaj nowego użytkownika.
            writer.write_all("HELLO".as_bytes()).await.unwrap();

            match reader.read_line(&mut line).await.unwrap() {
                0 => return,
                _ => {
                    let nick: String = line.clone().split_whitespace().collect();
                    if nick.is_empty() {
                        return;
                    }
                    let mut db = db.lock().await;
                    match db.contains_key(nick.as_str()) {
                        true => {
                            if db.get(nick.as_str()).unwrap().ip() == addr.ip() {
                                writer.write_all("HELLOAGAIN".as_bytes()).await.unwrap();
                                username = nick.to_string();
                            } else {
                                writer.write_all("USEREXISTS".as_bytes()).await.unwrap();

                                println!(
                                    "{} {} {}",
                                    format!("{:?}", addr).bold().red(),
                                    "próbował połączyć się jako".to_string().red(),
                                    format!("{:?}", nick.as_str()).bold().red(),
                                );
                                writer.shutdown().await.unwrap();
                            }
                        }
                        false => {
                            writer.write_all("HINEWCOMER".as_bytes()).await.unwrap();

                            println!(
                                "{} {} {}",
                                format!("{:?}", addr).bold().green(),
                                "połączył się jako".to_string().green(),
                                format!("{:?}", nick.as_str()).bold().green(),
                            );
                            db.insert(nick.to_string(), addr);
                            username = nick.to_string();

                            let message = json!({
                                "user": "Server",
                                "timestamp": Utc::now().timestamp(),
                                "style": 3,
                                "text": format!("{} joined the chat!", username)
                            });
                            tx.send((message.to_string(), addr)).unwrap();
                        }
                    }
                }
            }

            line.clear();

            // Obsługuj chatroom.
            loop {
                tokio::select! {
                    result = reader.read_line(&mut line) => {
                        if result.unwrap() == 0 {
                            break;
                        }
                        // Wysyłamy wiadomość jako JSON, który będzie odpowiednio formatowany przez klienta.
                        let message = json!({
                            "user": username,
                            "timestamp": Utc::now().timestamp(),
                            "style": 0,
                            "text": line.clone().trim()
                        });

                        if message.to_string().len() < MESSAGE_MAX_SIZE {
                            tx.send((message.to_string(), addr)).unwrap();
                        }
                        line.clear();
                    }
                    result = rx.recv() => {
                        let (msg, _addr) = result.unwrap();
                        let mut buff = msg.into_bytes();
                        buff.resize(MESSAGE_MAX_SIZE, 0b0);
                        writer.write_all( &buff).await.unwrap();
                    }
                }
            }
        });
    }
}
