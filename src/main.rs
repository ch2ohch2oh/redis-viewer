const PREVIEW_MAX_CHARS: usize = 40;

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
            println!("{key_type} {key} {preview}");
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
            Ok(truncate_preview(&value))
        }
        "list" => {
            let len: i64 = redis::cmd("LLEN").arg(key).query(connection)?;
            Ok(format!("len={len}"))
        }
        "set" => {
            let len: i64 = redis::cmd("SCARD").arg(key).query(connection)?;
            Ok(format!("len={len}"))
        }
        "hash" => {
            let len: i64 = redis::cmd("HLEN").arg(key).query(connection)?;
            Ok(format!("len={len}"))
        }
        "zset" => {
            let len: i64 = redis::cmd("ZCARD").arg(key).query(connection)?;
            Ok(format!("len={len}"))
        }
        "stream" => {
            let len: i64 = redis::cmd("XLEN").arg(key).query(connection)?;
            Ok(format!("len={len}"))
        }
        other => Ok(format!("({other})")),
    }
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
