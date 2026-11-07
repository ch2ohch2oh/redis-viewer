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

    match redis::cmd("PING").query::<String>(&mut connection) {
        Ok(reply) => println!("{reply}"),
        Err(error) => {
            eprintln!("PING failed: {error}");
            std::process::exit(1);
        }
    }
}
