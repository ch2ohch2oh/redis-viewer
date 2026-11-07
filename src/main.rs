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

    if let Err(error) = list_keys(&mut connection) {
        eprintln!("failed to list keys: {error}");
        std::process::exit(1);
    }
}

fn list_keys(connection: &mut redis::Connection) -> redis::RedisResult<()> {
    let mut cursor = 0u64;

    loop {
        let (next_cursor, keys): (u64, Vec<String>) =
            redis::cmd("SCAN").arg(cursor).query(connection)?;

        for key in keys {
            let key_type: String = redis::cmd("TYPE").arg(&key).query(connection)?;
            let preview = value_preview(connection, &key, &key_type)?;
            println!("{key_type:<TYPE_WIDTH$} {key} = {preview}");
        }

        if next_cursor == 0 {
            break;
        }

        cursor = next_cursor;
    }

    Ok(())
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
    let truncated = truncate_preview(value);
    let escaped = truncated.replace('\\', "\\\\").replace('"', "\\\"");
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
