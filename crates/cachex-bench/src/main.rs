

use cachex_bench::cli::{self, Config};

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("{}", cli::usage());
        return;
    }
    match Config::parse(&args) {
        Ok(config) => {
            if let Err(error) = cachex_bench::run(config).await {
                eprintln!("benchmark error: {error}");
                std::process::exit(1);
            }
        }
        Err(error) => {
            eprintln!("error: {error}");
            eprintln!();
            eprintln!("{}", cli::usage());
            std::process::exit(1);
        }
    }
}