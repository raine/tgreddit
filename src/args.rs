use getopts::Options;
use log::*;
use std::env;

pub fn parse_args() -> getopts::Matches {
    let args: Vec<String> = env::args().collect();
    let mut opts = Options::new();
    opts.optopt("", "debug-post", "", "");
    opts.optopt("", "chat-id", "", "");
    match opts.parse(&args[1..]) {
        Ok(m) => m,
        Err(f) => {
            error!("{}", f);
            std::process::exit(1);
        }
    }
}
