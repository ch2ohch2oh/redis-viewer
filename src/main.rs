const PREVIEW_MAX_CHARS: usize = 40;
const PREVIEW_MAX_ITEMS: usize = 3;
const TYPE_WIDTH: usize = 6;

fn main() {
    let client = match redis::Client::open("redis://127.0.0.1:6379/") {
        Ok(client) => client,
        Err(error) => {
            eprintln!("failed to create Redis client: {error}");
            std::process::exit(1);
        }
    };

    let mut connection = match client.get_connection() {
        Ok(connection) => connection,
        Err(error) => {
            eprintln!("failed to connect to Redis at 127.0.0.1:6379: {error}");
            std::process::exit(1);
        }
    };

    let rows = match load_rows(&mut connection) {
        Ok(rows) => rows,
        Err(error) => {
            eprintln!("failed to list keys: {error}");
            std::process::exit(1);
        }
    };

    if let Err(error) = run_tui(rows) {
        eprintln!("terminal error: {error}");
        std::process::exit(1);
    }
}

fn load_rows(connection: &mut redis::Connection) -> redis::RedisResult<Vec<String>> {
    let mut rows = Vec::new();
    let mut cursor = 0u64;

    loop {
        let (next_cursor, keys): (u64, Vec<String>) =
            redis::cmd("SCAN").arg(cursor).query(connection)?;

        for key in keys {
            let key_type: String = redis::cmd("TYPE").arg(&key).query(connection)?;
            let preview = value_preview(connection, &key, &key_type)?;
            rows.push(format!("{key_type:<TYPE_WIDTH$} {key} = {preview}"));
        }

        if next_cursor == 0 {
            break;
        }

        cursor = next_cursor;
    }

    Ok(rows)
}

fn run_tui(rows: Vec<String>) -> std::io::Result<()> {
    let mut terminal = ratatui::try_init()?;
    let result = run_app(&mut terminal, &rows);
    ratatui::restore();
    result
}

fn run_app(terminal: &mut ratatui::DefaultTerminal, rows: &[String]) -> std::io::Result<()> {
    use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
    use ratatui::widgets::ListState;

    let mut state = ListState::default();
    if !rows.is_empty() {
        state.select(Some(0));
    }

    loop {
        terminal.draw(|frame| draw(frame, rows, &mut state))?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                return Ok(());
            }
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                KeyCode::Down | KeyCode::Char('j') => state.select_next(),
                KeyCode::Up | KeyCode::Char('k') => state.select_previous(),
                KeyCode::PageDown => {
                    for _ in 0..10 {
                        state.select_next();
                    }
                }
                KeyCode::PageUp => {
                    for _ in 0..10 {
                        state.select_previous();
                    }
                }
                _ => {}
            }
        }
    }
}

fn draw(frame: &mut ratatui::Frame, rows: &[String], state: &mut ratatui::widgets::ListState) {
    use ratatui::layout::{Constraint, Layout};
    use ratatui::style::{Modifier, Style};
    use ratatui::widgets::{Block, List, ListItem};

    let chunks = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(frame.area());
    let items: Vec<ListItem> = if rows.is_empty() {
        vec![ListItem::new("(no keys)")]
    } else {
        rows.iter().map(|row| ListItem::new(row.as_str())).collect()
    };
    let list = List::new(items)
        .block(Block::bordered().title(" redis-viewer "))
        .highlight_style(Style::new().add_modifier(Modifier::REVERSED));
    frame.render_stateful_widget(list, chunks[0], state);
    frame.render_widget(
        ratatui::widgets::Paragraph::new("j/k or arrows scroll    q quit"),
        chunks[1],
    );
}

fn value_preview(
    connection: &mut redis::Connection,
    key: &str,
    key_type: &str,
) -> redis::RedisResult<String> {
    match key_type {
        "string" => {
            let value: Vec<u8> = redis::cmd("GET").arg(key).query(connection)?;
            let value = String::from_utf8_lossy(&value);
            Ok(quote_string(&truncate_preview(&value)))
        }
        "list" => {
            let len: i64 = redis::cmd("LLEN").arg(key).query(connection)?;
            let end = (PREVIEW_MAX_ITEMS as i64) - 1;
            let members: Vec<String> = redis::cmd("LRANGE")
                .arg(key)
                .arg(0)
                .arg(end)
                .query(connection)?;
            Ok(format_len_items(len, &members))
        }
        "set" => {
            let len: i64 = redis::cmd("SCARD").arg(key).query(connection)?;
            let (_cursor, members): (u64, Vec<String>) = redis::cmd("SSCAN")
                .arg(key)
                .arg(0)
                .arg("COUNT")
                .arg(PREVIEW_MAX_ITEMS)
                .query(connection)?;
            let members: Vec<String> = members.into_iter().take(PREVIEW_MAX_ITEMS).collect();
            Ok(format_len_items(len, &members))
        }
        "hash" => {
            let len: i64 = redis::cmd("HLEN").arg(key).query(connection)?;
            let (_cursor, entries): (u64, Vec<String>) = redis::cmd("HSCAN")
                .arg(key)
                .arg(0)
                .arg("COUNT")
                .arg(PREVIEW_MAX_ITEMS)
                .query(connection)?;
            let mut pairs = Vec::new();
            let mut entries = entries.into_iter();
            while pairs.len() < PREVIEW_MAX_ITEMS {
                match (entries.next(), entries.next()) {
                    (Some(field), Some(value)) => {
                        pairs.push(format!(
                            "{}={}",
                            truncate_preview(&field),
                            truncate_preview(&value)
                        ));
                    }
                    _ => break,
                }
            }
            Ok(format_len_items(len, &pairs))
        }
        "zset" => {
            let len: i64 = redis::cmd("ZCARD").arg(key).query(connection)?;
            let end = (PREVIEW_MAX_ITEMS as i64) - 1;
            let scored: Vec<(String, f64)> = redis::cmd("ZRANGE")
                .arg(key)
                .arg(0)
                .arg(end)
                .arg("WITHSCORES")
                .query(connection)?;
            let members: Vec<String> = scored
                .into_iter()
                .map(|(member, score)| format!("{}:{}", truncate_preview(&member), score))
                .collect();
            Ok(format_len_items(len, &members))
        }
        "stream" => {
            let len: i64 = redis::cmd("XLEN").arg(key).query(connection)?;
            let entries: Vec<(String, Vec<(String, String)>)> = redis::cmd("XRANGE")
                .arg(key)
                .arg("-")
                .arg("+")
                .arg("COUNT")
                .arg(PREVIEW_MAX_ITEMS)
                .query(connection)?;
            let ids: Vec<String> = entries.into_iter().map(|(id, _fields)| id).collect();
            Ok(format_len_items(len, &ids))
        }
        other => Ok(format!("({other})")),
    }
}

fn format_len_items(len: i64, items: &[String]) -> String {
    let mut shown: Vec<String> = items.iter().map(|item| truncate_preview(item)).collect();
    if len > shown.len() as i64 {
        shown.push("...".to_string());
    }
    format!("len={len} [{}]", shown.join(", "))
}

fn quote_string(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

fn truncate_preview(value: &str) -> String {
    let mut chars = value.chars();
    let truncated: String = chars.by_ref().take(PREVIEW_MAX_CHARS).collect();
    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        truncated
    }
}
