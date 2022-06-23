use colored::Colorize;
use crossterm::{
    event::{self, poll, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, read},
    execute,
    style::Stylize,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use num_derive::FromPrimitive;
use serde_json::{from_str, Value};

use std::{
    env,
    error::Error,
    io::{self, Read, Write},
    net::*,
    str::from_utf8,
    sync::mpsc::{self, TryRecvError},
    time::Duration,
};

use chrono::prelude::*;

const MESSAGE_MAX_LENGTH: usize = 256;
//                                    FPS
const MIN_UI_FRAMETIME: i64 = ((1.0 / 24.0) * 1000.0) as i64;

struct Message {
    timestamp: i64,
    sender: String,
    style: MessageStyle,
    text: String
}

struct Client {
    username: String,
    stream: TcpStream
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

    if args.len() != 3 {
        eprint!("Usage: <server address>:<port> <username>");
        return ;
    }

    let address = &args[1];
    let username = &mut args[2].clone();
    username.push('\n');

    if !(3..16).contains(&username.chars().count()) {
        eprint!("Username must be no longer than 16 and no shorter than 3");
        return ;
    }

    enable_raw_mode();
    let mut stdout = io::stdout();

    execute!(stdout, EnterAlternateScreen, EnableMouseCapture);

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
    let args: Vec<String> = env::args().collect();

    let mut stream = match TcpStream::connect(address) {
        Ok(s) => s,
        Err(e) => { return Err(e); }
    };

    let mut data = [0_u8; 5];
    match stream.read_exact(&mut data) {
        Ok(_) => {
            if &data != b"HELLO" {
                return Ok(
                    "Invalid server response (version mismatch? contact with server admin)"
                        .to_string()
                        .red()
                        .to_string(),
                );
            }
        }
        Err(e) => return Ok(format!("Server failed to respond\n{}", e).red().to_string()),
    }

    stream
        .write_all(username.as_bytes())
        .expect("Couldn't send data to server");

    let mut data = [0_u8; 10];
    match stream.read_exact(&mut data) {
        Ok(_) => match &data {
            b"HELLOAGAIN" | b"HINEWCOMER" => (),
            b"USEREXISTS" => {
                return Ok(format!("Username \"{}\" is already in use", username)
                    .purple()
                    .to_string())
            }
            _ => {
                return Ok(
                    "Invalid server response (version mismatch? contact with server admin)"
                        .to_string()
                        .red()
                        .to_string(),
                )
            }
        },
        Err(e) => return Ok(format!("Server failed to respond\n{}", e).red().to_string()),
    }

    stream
        .set_nonblocking(true)
        .expect("Unable to set stream as non-blocking");

    let (tx, rx) = mpsc::channel::<String>();
    let mut last_ui_update: i64 = 0;
    let mut do_ui_update: bool = true;

    loop {

        let now = Utc::now().timestamp_millis();
        let delta_t = now - last_ui_update;

        if do_ui_update && delta_t > MIN_UI_FRAMETIME {
            draw(&mut chat);
            last_ui_update = now;
            do_ui_update = false;
        }

        let mut header: [u8; 2] = [0, 0];

        if stream.read_exact(&mut header).is_ok() {
            let message_size = u16::from_be_bytes(header);
            
            let mut message_bytes = vec![0_u8; message_size.into()];
 
            if stream.read_exact(&mut message_bytes).is_ok() {
                let message_json = from_utf8(&message_bytes)
                    .expect("Received invalid UTF-8");
                
                let json_data: Value = from_str(message_json)
                    .expect("Received invalid data");

                let msg = Message {
                    timestamp: json_data["timestamp"].as_i64().unwrap(),
                    sender: json_data["sender"].as_str().unwrap().to_string(),
                    text: json_data["text"].as_str().unwrap().to_string(),
                    style: num::FromPrimitive::from_i64(json_data["style"].as_i64().unwrap()).unwrap()         
                };
                
                chat.log.push(msg);
                do_ui_update = true;
            }
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
