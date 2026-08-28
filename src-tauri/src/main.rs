// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use handy_app_lib::cli::CliParseOutcome;
use handy_app_lib::CliArgs;

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

    #[cfg(target_os = "linux")]
    {
        // DMABUF renderer causes crashes on various GPU/display server configurations
        // See: https://github.com/tauri-apps/tauri/issues/9394
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }

    handy_app_lib::run(cli_args)
}
