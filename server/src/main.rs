use chrono::*;
use futures::lock::Mutex;
use networking::*;
use serde_json::*;
use std::collections::HashMap;
use std::env;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::{TcpListener};
use tokio::sync::broadcast::{self};
use colored::Colorize;

pub mod networking;

type Db = Arc<Mutex<HashMap<String, SocketAddr>>>;

#[derive(Debug, Clone)]
enum MessageStyle {
    User = 0,
    Yourself = 1,
    //Admin = 2,
    Server = 3,
}

#[derive(Debug, Clone)]
pub struct Message {
    timestamp: i64,
    sender: String,
    style: MessageStyle,
    text: String,
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() != 2 {
        println!("Usage: <ip:port>");
        return;
    }

    println!("{} {}", 
        "Starting server on".bold(),
        &args[1].to_string().italic()
        );

    let listener;
    match TcpListener::bind(&args[1]).await {
        Ok(n) => {
            println!("{}", "Server up and running... ".green());
            listener = n;
        },
        Err(e) => {
            eprint!("{}\n{}",
                "Failed to start server, reason:".red(),
                e.to_string().italic()
                );
            return;
        }
    }

    let db: Db = Arc::new(Mutex::new(HashMap::new()));

    let (tx, _) = broadcast::channel(64);

    loop {
        let (mut socket, addr) = listener.accept().await.unwrap();
        
        let db = db.clone();
        let tx = tx.clone();
        let mut rx = tx.subscribe();
        
        tokio::spawn(async move {
            let (reader, mut writer) = socket.split();

            let mut reader = BufReader::new(reader);
            let mut line = String::new();
            let username;

            match receive_json(&mut reader).await {
                Some(v) => {
                    username = v["username"]
                        .as_str()
                        .expect("Failed to parse username")
                        .to_string();
                },
                None => return,
            }

            let mut db = db.lock().await;

            if db.contains_key(&username) {
                if db.get(&username)
                    .expect("Failed to access user database")
                    .ip() == addr.ip() {
                        send_json(json!({
                            "result": "ok",
                            "reason": "Welcome back"
                            }), &mut writer)
                            .await;
                            println!("{} {}", username.as_str(), "re-joined server".blue());
                    } else {
                        send_json(json!({
                            "result": "no",
                            "reason": "Username already taken"
                            }), &mut writer)
                            .await;
                            println!("{} {}", "Someone tried to join server as ".bright_red(), username.as_str());
                    }
            } else {
                send_json(json!({
                    "result": "ok",
                    "reason": "Welcome new user"
                    }), &mut writer)
                    .await;
                    
                    db.insert(username.to_string(), addr);
                    
                    let message = Message {
                        timestamp: Utc::now().timestamp(),
                        sender: "Server".to_string(),
                        style: MessageStyle::Server,
                        text: [username.as_str(), "joined chat"].join(" "),
                    };

                    tx.send((message, addr)).unwrap();
                    println!("{} {}", username.as_str(), "joined server".blue());
            }
            
            // we need to drop mutex guard manually beacuse scope is still alive
            drop(db); 

            line.clear();

            loop {
                tokio::select! {
                    result = reader.read_line(&mut line) => {
                        if result.unwrap() == 0 {
                            break;
                        }
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
