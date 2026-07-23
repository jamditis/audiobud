use clap::Parser;

#[derive(Parser, Debug, Clone, Default)]
#[command(name = "audiobud", about = "AudioBud - Speech to Text")]
pub struct CliArgs {
    /// Start with the main window hidden
    #[arg(long)]
    pub start_hidden: bool,

    /// Disable the system tray icon
    #[arg(long)]
    pub no_tray: bool,

    /// Toggle transcription on/off (sent to running instance)
    #[arg(long)]
    pub toggle_transcription: bool,

    /// Toggle transcription with post-processing on/off (sent to running instance)
    #[arg(long)]
    pub toggle_post_process: bool,

    /// Toggle raw transcription (lowercase, unpunctuated) on/off (sent to running instance)
    #[arg(long)]
    pub toggle_raw: bool,

    /// Cancel the current operation (sent to running instance)
    #[arg(long)]
    pub cancel: bool,

    /// Enable debug mode with verbose logging
    #[arg(long)]
    pub debug: bool,
}

#[cfg(test)]
mod tests {
    use super::CliArgs;
    use clap::CommandFactory;

    #[test]
    fn help_text_names_audiobud_not_the_upstream_fork() {
        let cmd = CliArgs::command();
        assert_eq!(cmd.get_name(), "audiobud");
        let about = cmd.get_about().expect("about is set").to_string();
        assert!(
            !about.contains("Handy"),
            "--help still names the upstream fork: {about:?}"
        );
        assert!(
            about.contains("AudioBud"),
            "--help omits the app name: {about:?}"
        );
    }
}
