use crossterm::{
    event::{self, poll, EnableMouseCapture, Event, KeyCode},
    execute,
    style::Stylize,
    terminal::{enable_raw_mode},
};

use num_derive::FromPrimitive;
use serde_json::*;

use std::{
    env,
    io::{self, Write},
    net::*,
    sync::mpsc::{self, TryRecvError},
    time::Duration,
};

use chrono::prelude::*;

pub mod networking;

use networking::*;

const MESSAGE_MAX_LENGTH: usize = 256;
const MIN_FRAMETIME: i64 = ((1.0 / 24.0) * 1000.0) as i64;

struct Message {
    timestamp: i64,
    sender: String,
    style: MessageStyle,
    text: String
}

struct Chat {
    log: Vec<Message>,
    input: String,
    input_mode: InputMode,
}

enum InputMode {
    Chatting,
    System,
}

#[derive(FromPrimitive)]
enum MessageStyle {
    User = 0,
    Yourself = 1,
    Admin = 2,
    Server = 3,
    Client = 4
}

impl Default for Chat {
    fn default() -> Chat {
        Chat {
            log: Vec::new(),
            input: String::new(),
            input_mode: InputMode::Chatting,
        }
    }
}

fn parse_message(m: &Message) -> String {
    let mut sender = m.sender.clone();
    let mut text = m.text.clone();

    match m.style {
        MessageStyle::User => {
            sender = sender.green().to_string();
            text = text.white().to_string();
        }
        MessageStyle::Yourself => {
            sender = sender.yellow().to_string();
            text = text.white().to_string();
        }
        MessageStyle::Admin => {
            sender = sender.magenta().to_string();
            text = text.white().to_string();
        }
        MessageStyle::Server => {
            sender = sender.on_blue().to_string();
            text = text.blue().to_string();
        }
        MessageStyle::Client => {
            sender = sender.on_white().to_string();
            text = text.white().bold().to_string();
        }
    };

    return format!(
        "[{}] {}: {}",
        Utc.timestamp(m.timestamp, 0).format("%H:%M").to_string().grey().italic(),
        sender,
        text
        );
}

fn main() {
    

    let args: Vec<String> = env::args().collect();

    if args.len() != 4 {
        eprint!("Usage: <server address>:<port> <username> <room>");
        return ;
    }

    let address = &args[1];
    let username = &mut args[2].clone();


    if !(3..16).contains(&username.chars().count()) {
        eprint!("Username must be no longer than 16 and no shorter than 3");
        return ;
    }

    enable_raw_mode()
        .expect("Crossterm: Failed to enable raw mode");
    let mut stdout = io::stdout();

    execute!(stdout, EnableMouseCapture)
        .expect("Crossterm: Failed to execute command preset");

    let app = Chat::default();
    match run_app(app, address.to_string(), username.to_string()) {
        Ok(r) => {
            println!("{}", r);
        },
        Err(e) => {
            eprint!("Error occured: {}", e)
        }
    } 
}

fn run_app(mut chat: Chat, address: String, username: String) -> io::Result<String> {

    let mut stream = match TcpStream::connect(address) {
        Ok(s) => s,
        Err(e) => { return Err(e); }
    };

    ////////

    send_json(json!({
        "username": username
    }), &mut stream);

    ////////

    match receive_json(&mut stream) {
        Some(v) => {
            match v["result"].as_str().unwrap() {
                "ok" => {
                    println!("{}", "Successfully joined chat".on_green());
                },
                "no" => {
                   return Ok(v["reason"].as_str().unwrap().to_string());
                },
                _ => {
                    return Ok("Unexpected server response".to_string());
                }
            }
        },
        None => todo!(),
    }

    ///////

    stream
        .set_nonblocking(true)
        .expect("Unable to set stream as non-blocking");

    let (tx, rx) = mpsc::channel::<String>();
    let mut last_ui_update: i64 = 0;
    let mut do_ui_update: bool = true;

    loop {

        let now = Utc::now().timestamp_millis();
        let delta_t = now - last_ui_update;

        if do_ui_update && delta_t > MIN_FRAMETIME {
            draw(&mut chat);
            last_ui_update = now;
            do_ui_update = false;
        }

        match receive_json(&mut stream) {
            Some(data) => {
                let msg = Message {
                    timestamp: data["timestamp"].as_i64().unwrap(),
                    sender: data["sender"].as_str().unwrap().to_string(),
                    text: data["text"].as_str().unwrap().to_string(),
                    style: num::FromPrimitive::from_i64(data["style"].as_i64().unwrap()).unwrap()         
                };

                chat.log.push(msg);
                do_ui_update = true;
            },
            None => {},
        }
        
        match rx.try_recv() {
            Ok(mut data) => {
                data.push('\n');
                stream
                    .write_all(data.as_bytes())
                    .expect("Unable to send data to server");
            }
            Err(TryRecvError::Empty) => (),
            Err(TryRecvError::Disconnected) => {
                return Ok("Connection with server unexpectedly closed"
                    .to_string()
                    .red()
                    .to_string())
            }
        }

        if poll(Duration::from_millis(0)).unwrap() {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Enter => {
                        match chat.input_mode {
                            InputMode::Chatting => {
                                tx.send(chat.input.drain(..).collect()).unwrap();
                                //chat.log.push(chat.input.drain(..).collect());
                            }
                            InputMode::System => {
                                match chat.input.to_lowercase().as_str() {
                                    "exit" | "e" | "quit" | "q" => {
                                        return Ok("Bye".to_string());
                                    }
                                    "help" | "?" => chat.log.push(Message {
                                        timestamp: Utc::now().timestamp(),
                                        sender: "System".to_string(),
                                        text: "(E)xit, (Q)uit - closes app".to_string(),
                                        style: MessageStyle::Client,
                                    }),
                                    _ => chat.log.push(Message {
                                        timestamp: Utc::now().timestamp(),
                                        sender: "System".to_string(),
                                        text: "Unknown command".to_string(),
                                        style: MessageStyle::Client,
                                    }),
                                }
                                chat.input.clear();
                            }
                        }
                    }
                    KeyCode::Char(c) => {
                        if chat.input.chars().count() < MESSAGE_MAX_LENGTH {
                            chat.input.push(c);
                        }
                    }
                    KeyCode::Backspace => {
                        chat.input.pop();
                    }
                    KeyCode::Esc => {
                        chat.input.clear();
                        chat.input_mode = match chat.input_mode {
                            InputMode::Chatting => InputMode::System,
                            InputMode::System => InputMode::Chatting,
                        };
                    }
                    _ => {}
                }
                do_ui_update = true;
            }
        }

    }
}

fn draw(chat: &mut Chat) {
    let input_label = match chat.input_mode {
        InputMode::Chatting => "CHAT>".to_string().on_white(), 
        InputMode::System => "COMMAND>".to_string().on_magenta(), 
    };

    let messages = chat.log.drain(..).rev();
    let terminal_width = crossterm::terminal::size().unwrap().0.into();
    // line_capacity is how much characters we can display in input line,
    // -1 because there is 1-chat wide space between Label and actual input text 
    let line_capacity = terminal_width - input_label.content().chars().count() - 1;

    print!("\r{}\r", " ".repeat(terminal_width));

    if messages.len() > 0 {
        for m in messages {
            print!("{}\n\r", parse_message(&m));
        }
    } 

    let mut input_line = chat.input.clone();
    let input_length = input_line.chars().count();

    if input_length > line_capacity {
        input_line = input_line.split_at(input_length-line_capacity).1.to_string();
    }

    if input_length >= MESSAGE_MAX_LENGTH {
        input_line = input_line.red().to_string();
    }

    print!("{} {}", input_label, input_line);
    io::stdout().flush().unwrap();
}
