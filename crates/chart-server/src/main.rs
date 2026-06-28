mod config;
use clap::Parser;

fn main() {
    let cfg = config::Config::parse();
    println!("listening on {}:{}", cfg.host, cfg.port);
}
