// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use audiobud_lib::cli::CliParseOutcome;
use audiobud_lib::CliArgs;

fn main() {
    let cli_args = match CliArgs::parse_env() {
        Ok(CliParseOutcome::Run(arguments)) => arguments,
        Ok(CliParseOutcome::Help) => {
            print!("{}", CliArgs::help());
            return;
        }
        Ok(CliParseOutcome::Version) => {
            println!("{}", CliArgs::version());
            return;
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };

    audiobud_lib::run(cli_args)
}
