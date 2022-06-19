use chrono::*;
use colored::*;
use futures::lock::Mutex;
use serde_json::*;
use tokio::net::tcp::WriteHalf;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::sync::broadcast;
//use openssl::rsa::*; TODO szyfrowanie

type Db = Arc<Mutex<HashMap<String, SocketAddr>>>;

//const MESSAGE_MAX_SIZE: usize = 512;

#[derive(Debug, Clone)]
enum ChatMessageStyle {
    User = 0,
    Yourself = 1,
    Admin = 2,
    Server = 3,
}

#[derive(Debug, Clone)]
struct ChatMessage {
    timestamp: i64,
    sender: String,
    style: ChatMessageStyle,
    text: String,    
}

async fn send_message(data: ChatMessage, s: &mut WriteHalf<'_>) {
    let data_json = json!({
        "timestamp": data.timestamp,
        "sender": data.sender,
        "style": data.style as u8,
        "text": data.text
    }).to_string();

    let header: [u8; 2] = (data_json.len() as u16).to_be_bytes();
    s.write_all(&[&header, data_json.as_bytes()].concat()).await.unwrap();
} 

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

                            let message = ChatMessage {
                                timestamp: Utc::now().timestamp(),
                                sender: "Server".to_string(),
                                style: ChatMessageStyle::Server,
                                text: [nick.as_str(), "joined chat"].join(" ")
                            };
                            tx.send((message, addr)).unwrap();
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
                        let message = ChatMessage {
                            timestamp: Utc::now().timestamp(),
                            sender: username.clone(),
                            style: ChatMessageStyle::User,
                            text: line.clone().trim().to_string()
                        };

                        tx.send((message, addr)).unwrap();
                        line.clear();
                    }
                    result = rx.recv() => {
                        let (mut msg, _addr) = result.unwrap();
                        if msg.sender == username {
                            msg.style = ChatMessageStyle::Yourself
                        }
                        send_message(msg, &mut writer).await
                    }
                }
            }
        });
    }
}
