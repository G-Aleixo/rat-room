use std::{io, time::Duration};
use color_eyre::eyre::Result;
use crossterm::{event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind}, execute, terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode}};
use rat_room::message::Message;
use ratatui::{Terminal, layout::{Constraint, Direction, Layout, Position}, prelude::CrosstermBackend, widgets::{Block, Borders, Paragraph}};
use tokio::{
    io::{
        BufReader
    },
    net::TcpStream, sync::mpsc,
};

struct App {
    messages: Vec<String>,
    input: String,
    max_messages: usize,
    user_name: String,
}

impl App {
    fn new() -> Self {
        Self {
            messages: vec![
                "Welcome to rat room".into(),
                "Type in the box and press enter to send a message".into()
            ],
            input: String::new(),
            max_messages: 128,
            user_name: "no_name".into(),
        }
    }

    fn push_message(&mut self, msg: String) {
        self.messages.push(msg);
        if self.messages.len() > self.max_messages {
            self.messages.remove(0); // drop last message
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <name> <server_addr>", args[0]);
        std::process::exit(1);
    }
    let name = args[1].clone();
    let server_addr = args[2].clone();

    let stream = TcpStream::connect(server_addr).await?;
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    // recv messages task
    let (tx_net, rx_ui) = mpsc::channel::<Message>(100);
    tokio::spawn(async move {
        loop {
            match Message::read(&mut reader).await {
                Ok(msg) => {
                    if tx_net.send(msg).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    // send messages task
    let (tx_ui, mut rx_net) = mpsc::channel::<Message>(100);
    tokio::spawn(async move {
        while let Some(msg) = rx_net.recv().await {
            if let Err(e) = msg.write(&mut writer).await {
                eprintln!("Failed to send message: {e}");
                break;
            }
        }
    });

    // ui task
    tokio::task::spawn_blocking(move || -> Result<()>{
        // setup terminal
        enable_raw_mode()?;

        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = ratatui::backend::CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        // create and run app
        let mut app = App::new();

        app.user_name = name;

        let res = run(&mut terminal, &mut app, rx_ui, tx_ui);

        // restore terminal
        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
        terminal.show_cursor()?;

        // throw error if response was error
        res?;

        Ok(())
    }).await??;
    
    Ok(())
}

fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App, mut rx: mpsc::Receiver<Message>, tx_ui: mpsc::Sender<Message>) -> Result<()>{
    loop {
        while let Ok(msg) = rx.try_recv() {
            app.push_message(format!("{}", msg));
        }

        terminal.draw(|f| {
            // (d-2) * (width-2) >= 510
            let input_height = (255 + app.user_name.len() + 3) / (f.area().width - 2) as usize + 3;

            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints(vec![
                    Constraint::Min(0),
                    Constraint::Length(input_height as u16)
                ])
                .split(f.area());

            // use custom wrap method since the ratatui broke the height getting
            let wrapped_messages = wrap_messages(&app.messages, chunks[0].width.saturating_sub(2).into());

            // amount-size
            let visible_messages = &wrapped_messages[wrapped_messages.len().saturating_sub(chunks[0].height.saturating_sub(2) as usize)..];
            
            let message_text = visible_messages.join("\n");
            let messages = Paragraph::new(message_text)
                .block(Block::default().title("Messages").borders(Borders::ALL));

            f.render_widget(messages, chunks[0]);

            let wrapped_input = wrap_messages(&vec![app.user_name.clone() + " > " + &app.input.clone()], chunks[1].width.saturating_sub(2).into());
            
            let input_display = wrapped_input.join("\n");
            
            let input = Paragraph::new(input_display)
                .block(Block::default().title("Input").borders(Borders::ALL));

            f.render_widget(input, chunks[1]);

            // Cursor placement
            let line = wrapped_input.len() as u16 - 1;

            let col = wrapped_input[wrapped_input.len()-1].len() as u16;
                    
            // hot fix for case "name > ", where the cursor is incorrectly placed one char after the ">" instead of 2  
            let col = if col == app.user_name.len() as u16 + 2 {
                col + 1
            } else {
                col
            };

            let cursor_x = chunks[1].x + 1 + col;   // inside left border
            let cursor_y = chunks[1].y + 1 + line;  // move down for wrapped lines
            f.set_cursor_position(Position::new(cursor_x, cursor_y));

        })?;

        if event::poll(Duration::from_millis(200))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                    KeyCode::Char(c) => {
                        if app.input.len() < u8::MAX.into() {
                            app.input.push(c)
                        }
                    }
                    KeyCode::Backspace => { app.input.pop(); } // don't return
                    KeyCode::Enter => {
                        if !app.input.trim().is_empty() {
                            let msg = Message::new(app.user_name.clone(), app.input.clone())
                                .map_err(|e| color_eyre::eyre::eyre!(e))?;
                            // push current message to buffer and clear messages
                            app.push_message(msg.to_string());

                            //TODO: fix error when server disconnects or goes offline
                            tx_ui.blocking_send(msg)?;

                            app.input.clear();
                        }
                    }
                    KeyCode::Esc => break,
                    _ => {}
                }

                _ => {}
            }
        }
    }

    Ok(())
}

// use textwrap to wrap the vec into another vector
fn wrap_messages(messages: &Vec<String>, width: usize) -> Vec<String> {
    // Allocate at least the amount of messages needed
    let mut cows: Vec<std::borrow::Cow<str>> = Vec::with_capacity(messages.len());

    let wrap_config = textwrap::Options::new(width)
        .word_splitter(textwrap::WordSplitter::NoHyphenation)
        .break_words(true);

    for message in messages {
        cows.extend(textwrap::wrap(message.as_str(), &wrap_config));
    };

    let wrapped = cows.into_iter()
        .map(|c| c.into_owned())
        .collect();

    return wrapped;
}