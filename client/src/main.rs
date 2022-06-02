use colored::Colorize;
use crossterm::{
    event::{self, poll, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    style::Stylize,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
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

const MESSAGE_MAX_LENGTH: u8 = 64;
const MESSAGE_MAX_SIZE: usize = 512;

struct Message {
    timestamp: i64,
    sender: String,
    text: String,
    style: i64, // to będzie trzeba zmienić na u8
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

impl Default for Chat {
    fn default() -> Chat {
        Chat {
            log: Vec::new(),
            input: String::new(),
            input_mode: InputMode::Chatting,
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
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

    // glowny loop chatu
    loop {
        terminal.draw(|f| ui(f, &mut chat))?;

        let mut buff = vec![0_u8; MESSAGE_MAX_SIZE];
        if stream.read_exact(&mut buff).is_ok() {
            let data = from_utf8(&buff)
                .expect("Received invalid UTF-8")
                .trim_matches(char::from(0));
            let json_data: Value = from_str(data).expect("Received invalid data");

            let mut msg = Message {
                timestamp: json_data["timestamp"].as_i64().unwrap(),
                sender: json_data["user"].as_str().unwrap().to_string(),
                text: json_data["text"].as_str().unwrap().to_string(),
                style: json_data["style"].as_i64().unwrap(),
            };

            if msg.sender.eq(username.trim()) {
                msg.style = 1;
            }
            //
            chat.log.push(msg);
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
                                        style: 2,
                                    }),
                                    _ => chat.log.push(Message {
                                        timestamp: Utc::now().timestamp(),
                                        sender: "System".to_string(),
                                        text: "Unknown command".to_string(),
                                        style: 2,
                                    }),
                                }
                                chat.input.clear();
                            }
                        }
                    }
                    KeyCode::Char(c) => {
                        if chat.input.chars().count() < 64 {
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
            }
        }
    }
}

fn ui<B: Backend>(f: &mut Frame<B>, chat: &mut Chat) {
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

    let messages: Vec<ListItem> = chat
        .log
        .iter()
        .enumerate()
        .map(|(_, m)| {
            let name_style = match m.style {
                1 => Style::default().fg(Color::Yellow),
                2 => Style::default().bg(Color::White),
                3 => Style::default().bg(Color::Green),
                _ => Style::default().fg(Color::Green),
            };

            let text_style = match m.style {
                2 => Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
                3 => Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
                _ => Style::default().fg(Color::White),
            };

            let naive = NaiveDateTime::from_timestamp(m.timestamp, 0);
            let dt: DateTime<Utc> = DateTime::from_utc(naive, Utc);
            let date = dt.format("%H:%M:%S");

            let content = vec![Spans::from(vec![
                Span::styled(
                    format!("[{}] ", date),
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::ITALIC),
                ),
                Span::styled(format!("{}:", m.sender.clone()), name_style),
                Span::styled(format!(" {}", m.text.clone()), text_style),
            ])];
            ListItem::new(content)
        })
        .collect();

    // cały ten blok do optymalizacji (dodac przewijanie wiadomosci?)
    let mut log_capacity = chunks[0].height - 2;
    log_capacity = log_capacity.clamp(1, 24);
    if chat.log.len() > log_capacity.into() {
        chat.log.remove(0); // O(n)
    }
    //

    let messages =
        List::new(messages).block(Block::default().borders(Borders::ALL).title("Chat log"));
    f.render_widget(messages, chunks[0]);

    let (msg, style) = (
        vec![
            Span::raw("Press "),
            Span::styled("[Esc]", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" to switch between command and chatting mode. Type "),
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
            InputMode::System => Style::default().fg(Color::LightYellow),
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
