const PREVIEW_MAX_ITEMS: usize = 3;

struct KeyRow {
    key_type: String,
    key: String,
    preview: String,
}

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

fn load_rows(connection: &mut redis::Connection) -> redis::RedisResult<Vec<KeyRow>> {
    let mut rows = Vec::new();
    let mut cursor = 0u64;

    loop {
        let (next_cursor, keys): (u64, Vec<String>) =
            redis::cmd("SCAN").arg(cursor).query(connection)?;

        for key in keys {
            let key_type: String = redis::cmd("TYPE").arg(&key).query(connection)?;
            let preview = value_preview(connection, &key, &key_type)?;
            rows.push(KeyRow {
                key_type,
                key,
                preview,
            });
        }

        if next_cursor == 0 {
            break;
        }

        cursor = next_cursor;
    }

    Ok(rows)
}

fn run_tui(rows: Vec<KeyRow>) -> std::io::Result<()> {
    let mut terminal = ratatui::try_init()?;
    let result = run_app(&mut terminal, &rows);
    ratatui::restore();
    result
}

fn run_app(terminal: &mut ratatui::DefaultTerminal, rows: &[KeyRow]) -> std::io::Result<()> {
    use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
    use ratatui::widgets::TableState;

    let mut state = TableState::default();
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

fn draw(frame: &mut ratatui::Frame, rows: &[KeyRow], state: &mut ratatui::widgets::TableState) {
    use ratatui::layout::{Constraint, Layout};
    use ratatui::style::{Modifier, Style};
    use ratatui::widgets::{Block, Cell, Row, Table};

    let chunks = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(frame.area());
    let inner_width = chunks[0].width.saturating_sub(2);
    let [type_width, key_width, preview_width] = column_widths(inner_width);
    let table_rows: Vec<Row> = if rows.is_empty() {
        vec![Row::new([
            String::new(),
            fit("(no keys)", key_width),
            String::new(),
        ])]
    } else {
        rows.iter()
            .map(|row| {
                Row::new([
                    Cell::from(fit(&row.key_type, type_width)),
                    Cell::from(fit(&row.key, key_width)),
                    Cell::from(fit(&row.preview, preview_width)),
                ])
            })
            .collect()
    };
    let table = Table::new(
        table_rows,
        [
            Constraint::Length(type_width),
            Constraint::Length(key_width),
            Constraint::Length(preview_width),
        ],
    )
    .header(
        Row::new([
            fit("type", type_width),
            fit("key", key_width),
            fit("preview", preview_width),
        ])
        .style(Style::new().add_modifier(Modifier::BOLD)),
    )
    .block(Block::bordered().title(" redis-viewer "))
    .row_highlight_style(Style::new().add_modifier(Modifier::REVERSED));
    frame.render_stateful_widget(table, chunks[0], state);
    frame.render_widget(
        ratatui::widgets::Paragraph::new("j/k or arrows scroll    q quit"),
        chunks[1],
    );
}

fn column_widths(inner_width: u16) -> [u16; 3] {
    const COLUMN_GAPS: u16 = 2;
    let usable = inner_width.saturating_sub(COLUMN_GAPS);
    let type_width = 6.min(usable);
    let rest = usable.saturating_sub(type_width);
    let key_width = rest.saturating_mul(2) / 5;
    let preview_width = rest.saturating_sub(key_width);
    [type_width, key_width, preview_width]
}

fn fit(value: &str, width: u16) -> String {
    let width = usize::from(width);
    if width == 0 {
        return String::new();
    }
    if unicode_width::UnicodeWidthStr::width(value) <= width {
        return value.to_string();
    }
    const ELLIPSIS: &str = "...";
    if width <= ELLIPSIS.len() {
        return ELLIPSIS.chars().take(width).collect();
    }
    let mut kept = String::new();
    let mut used = 0;
    for ch in value.chars() {
        let ch_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + ch_width + ELLIPSIS.len() > width {
            break;
        }
        kept.push(ch);
        used += ch_width;
    }
    kept.push_str(ELLIPSIS);
    kept
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
            Ok(quote_string(&value))
        }
        "list" | "set" | "hash" | "zset" | "stream" => {
            collection_preview(connection, key, key_type)
        }
        other => Ok(format!("({other})")),
    }
}

fn collection_preview(
    connection: &mut redis::Connection,
    key: &str,
    key_type: &str,
) -> redis::RedisResult<String> {
    let end = (PREVIEW_MAX_ITEMS as i64) - 1;
    let (len_command, samples): (&str, Vec<String>) = match key_type {
        "list" => {
            let samples = redis::cmd("LRANGE")
                .arg(key)
                .arg(0)
                .arg(end)
                .query(connection)?;
            ("LLEN", samples)
        }
        "set" => {
            let (_cursor, members): (u64, Vec<String>) = redis::cmd("SSCAN")
                .arg(key)
                .arg(0)
                .arg("COUNT")
                .arg(PREVIEW_MAX_ITEMS)
                .query(connection)?;
            (
                "SCARD",
                members.into_iter().take(PREVIEW_MAX_ITEMS).collect(),
            )
        }
        "hash" => {
            let (_cursor, fields): (u64, Vec<String>) = redis::cmd("HSCAN")
                .arg(key)
                .arg(0)
                .arg("COUNT")
                .arg(PREVIEW_MAX_ITEMS)
                .query(connection)?;
            let samples = fields
                .chunks(2)
                .take(PREVIEW_MAX_ITEMS)
                .filter_map(|pair| match pair {
                    [field, value] => Some(format!("{field}={value}")),
                    _ => None,
                })
                .collect();
            ("HLEN", samples)
        }
        "zset" => {
            let scored: Vec<(String, f64)> = redis::cmd("ZRANGE")
                .arg(key)
                .arg(0)
                .arg(end)
                .arg("WITHSCORES")
                .query(connection)?;
            let samples = scored
                .into_iter()
                .map(|(member, score)| format!("{member}:{score}"))
                .collect();
            ("ZCARD", samples)
        }
        "stream" => {
            let entries: Vec<(String, Vec<(String, String)>)> = redis::cmd("XRANGE")
                .arg(key)
                .arg("-")
                .arg("+")
                .arg("COUNT")
                .arg(PREVIEW_MAX_ITEMS)
                .query(connection)?;
            let samples = entries.into_iter().map(|(id, _fields)| id).collect();
            ("XLEN", samples)
        }
        other => return Ok(format!("({other})")),
    };
    let len: i64 = redis::cmd(len_command).arg(key).query(connection)?;
    Ok(format_len_items(len, &samples))
}

fn format_len_items(len: i64, items: &[String]) -> String {
    let mut shown = items.to_vec();
    if len > shown.len() as i64 {
        shown.push("...".to_string());
    }
    format!("len={len} [{}]", shown.join(", "))
}

fn quote_string(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}
