use colored::Colorize;
use crossterm::{
    event::{self, poll, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
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
use tui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Span, Spans, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame, Terminal,
};
use unicode_width::UnicodeWidthStr;

const MESSAGE_MAX_LENGTH: u8 = 50;
//                                    FPS
const MIN_UI_FRAMETIME: i64 = ((1.0 / 60.0) * 1000.0) as i64;
const MAX_UI_FRAMETIME: i64 = ((1.0 / 5.0) * 1000.0) as i64;

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

fn parse_message(m: &Message) -> ListItem<'static> {
    let name_style = match m.style {
        MessageStyle::User => Style::default()
            .fg(Color::Green),

        MessageStyle::Yourself => Style::default()
            .fg(Color::Yellow),

        MessageStyle::Admin => Style::default()
            .fg(Color::LightMagenta),

        MessageStyle::Server => Style::default()
            .bg(Color::Blue),

        MessageStyle::Client => Style::default()
            .bg(Color::White)
    };

    let text_style = match m.style {        
        MessageStyle::Server => Style::default()
            .fg(Color::Blue)
            .add_modifier(Modifier::BOLD),

        MessageStyle::Client => Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),

        _ => Style::default()
            .fg(Color::White)
    };

    let date = Utc.timestamp(m.timestamp, 0)
        .format("%H:%M ");

    let content = Text::from(
            Spans::from(
                vec![
                    Span::styled(date.to_string(),
                        Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::ITALIC)),
                    Span::styled(format!("{}:", m.sender.clone()), name_style),
                    Span::raw(" "),
                    Span::styled(m.text.clone(), text_style)
                ]
            )
        );
    
    ListItem::new(content)
}

fn main() -> Result<(), Box<dyn Error>> {
    //let args: Vec<String> = env::args().collect();

    //if args.len != 

    enable_raw_mode()?;
    let mut stdout = io::stdout();

    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let app = Chat::default();
    let out = run_app(&mut terminal, app);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;

    terminal.show_cursor()?;

    match out {
        Ok(msg) => println!("{}", msg),
        Err(err) => println!("{:?}", err),
    }

    Ok(())
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>, mut chat: Chat) -> io::Result<String> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 3 {
        return Ok("Too few arguments, usage: <server address> <username>".to_string());
    }

    let address = &args[1];
    let username = &mut args[2].clone();
    username.push('\n'); // potrzebujemy tego newlina zeby serwer poprawie odebral username

    if !(3..16).contains(&username.chars().count()) {
        return Ok(
            "Username must be at least 3 and at most 16 characters long (2nd argument)".to_string(),
        );
    }

    let mut stream = match TcpStream::connect(address) {
        Ok(s) => s,
        Err(e) => {
            return Ok(format!("Unable to connect with server\n{}", e)
                .yellow()
                .to_string())
        }
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

        if (do_ui_update && delta_t > MIN_UI_FRAMETIME) || delta_t > MAX_UI_FRAMETIME {
            terminal.draw(|f| draw_ui(f, &mut chat))?;
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
                        if chat.input.chars().count() < MESSAGE_MAX_LENGTH.into() {
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

fn draw_ui<B: Backend>(f: &mut Frame<B>, chat: &mut Chat) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints(
            [
                Constraint::Min(1),
                Constraint::Length(1),
                Constraint::Length(3),
            ]
            .as_ref(),
        )
        .split(f.size());

    if f.size().height < 24 || f.size().width < 80 {
        f.render_widget(Paragraph::new(Text::styled(
                                    "Please resize terminal to at \nleast 80 columns and 24 rows ", 
                                    Style::default()
                                    .bg(Color::Red)
                                    .fg(Color::White)
                                    .add_modifier(Modifier::BOLD)
                                    )), chunks[0]);
        return;
    }

    let messages: Vec<ListItem>;
    
    let display_capacity = (chunks[0].height - 2) as usize;
    while chat.log.len() > display_capacity {
        chat.log.remove(0);
    }

    messages = 
            chat.log
            .iter()
            .enumerate()
            .map(|(_, m)| {
                parse_message(m)
            }).collect();

    let messages =
        List::new(messages).block(Block::default().borders(Borders::ALL).title("Chat log"));
    f.render_widget(messages, chunks[0]);

    let (msg, style) = (
        vec![
            Span::raw(""),
            Span::styled("[Esc]", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" - switch modes. Type "),
            Span::styled("\"?\"", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" in command mode for help."),
        ],
        Style::default(),
    );
    let mut text = Text::from(Spans::from(msg));
    text.patch_style(style);
    let help_message = Paragraph::new(text);
    f.render_widget(help_message, chunks[1]);

    let input = Paragraph::new(chat.input.as_ref())
        .style(match chat.input_mode {
            InputMode::Chatting => Style::default(),
            InputMode::System => Style::default().fg(Color::LightMagenta),
        })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(match chat.input_mode {
                    InputMode::Chatting => format!(
                        "Chat ({}/{})",
                        chat.input.chars().count(),
                        MESSAGE_MAX_LENGTH
                    ),
                    InputMode::System => "Command".to_string(),
                }),
        );
    f.render_widget(input, chunks[2]);
    f.set_cursor(
        // Put cursor past the end of the input text
        chunks[1].x + chat.input.width() as u16 + 1,
        // Move one line down, from the border to the input line
        chunks[1].y + 2,
    )
}
