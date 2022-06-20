use chrono::*;
use colored::Colorize;
use futures::lock::Mutex;
use serde_json::*;
use tokio::time::timeout;
use std::collections::HashMap;
use std::env;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::tcp::WriteHalf;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
//use openssl::rsa::*; TODO szyfrowanie

type Db = Arc<Mutex<HashMap<String, SocketAddr>>>;

#[derive(Debug, Clone)]
enum MessageStyle {
    User = 0,
    Yourself = 1,
    Admin = 2,
    Server = 3,
}

#[derive(Debug, Clone)]
struct Message {
    timestamp: i64,
    sender: String,
    style: MessageStyle,
    text: String,
}

enum MessageType {
    Hello = 0,
    HelloAgain = 1,
    UserExists = 2,
    HiNewComer = 3,
}

impl MessageType {
    fn as_str(&self) -> &'static str {
        match self {
            MessageType::Hello => "HELLO",
            MessageType::HelloAgain => "HELLOAGAIN",
            MessageType::UserExists => "USEREXISTS",
            MessageType::HiNewComer => "HINEWCOMER",
        }
    }
}

async fn send_message(data: Message, s: &mut WriteHalf<'_>) {
    let data_json = json!({
        "timestamp": data.timestamp,
        "sender": data.sender,
        "style": data.style as u8,
        "text": data.text
    })
    .to_string();

    if data_json.len() > 65535 {
        println!("Wiadomość za długa");
        return;
        // todo - need better handling 
    }

    let header: [u8; 2] = (data_json.len() as u16).to_be_bytes();
    s.write_all(&[&header, data_json.as_bytes()].concat())
        .await
        .unwrap();   
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() != 2 {
        println!("Usage: <port>");
        return;
    }

    let mut host: String = "localhost:".to_owned();
    host.push_str(&args[1]);
    let listener = TcpListener::bind(host).await.unwrap();

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
            writer
                .write_all(MessageType::Hello.as_str().as_bytes())
                .await
                .unwrap();

            match reader.read_line(&mut line).await.unwrap() {
                0 => return,
                _ => { // TODO - rewrite this section, client should pass all data in one batch
                    let nick: String = line.clone().split_whitespace().collect();
                    if nick.is_empty() {
                        return;
                    }
                    let mut db = db.lock().await;
                    match db.contains_key(nick.as_str()) {
                        true => {
                            if db.get(nick.as_str()).unwrap().ip() == addr.ip() {
                                writer
                                    .write_all(MessageType::HelloAgain.as_str().as_bytes())
                                    .await
                                    .unwrap();
                                username = nick.to_string();
                            } else {
                                writer
                                    .write_all(MessageType::UserExists.as_str().as_bytes())
                                    .await
                                    .unwrap();

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
                            writer
                                .write_all(MessageType::HiNewComer.as_str().as_bytes())
                                .await
                                .unwrap();

                            println!(
                                "{} {} {}",
                                format!("{:?}", addr).bold().green(),
                                "połączył się jako".to_string().green(),
                                format!("{:?}", nick.as_str()).bold().green(),
                            );

                            db.insert(nick.to_string(), addr);
                            username = nick.to_string();

                            let message = Message {
                                timestamp: Utc::now().timestamp(),
                                sender: "Server".to_string(),
                                style: MessageStyle::Server,
                                text: [nick.as_str(), "joined chat"].join(" "),
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
                        let message = Message {
                            timestamp: Utc::now().timestamp(),
                            sender: username.clone(),
                            style: MessageStyle::User,
                            text: line.clone().trim().to_string()
                        };

                        tx.send((message, addr))
                            .unwrap();

                        line.clear();
                    }
                    result = rx.recv() => {
                        let (mut msg, _addr) = result.unwrap();

                        if msg.sender == username {
                            msg.style = MessageStyle::Yourself
                        }

                        send_message(msg, &mut writer)
                            .await;
                    }
                }
            }
        });
    }
}
