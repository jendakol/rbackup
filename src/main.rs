extern crate rbackup;

use std::process;
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() != 2 {
        eprintln!("No filename was provided!");
        process::exit(1);
    }

    let file_name = &args[1];

    match rbackup::run(file_name) {
        Ok(()) => {
            println!();
            ()
        }
        Err(e) => {
            eprintln!("{}", e);
            process::exit(1);
        }
    }
}
