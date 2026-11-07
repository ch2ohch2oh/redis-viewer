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
            println!("{key_type} {key}");
        }

        if next_cursor == 0 {
            break;
        }

        cursor = next_cursor;
    }

    Ok(())
}
