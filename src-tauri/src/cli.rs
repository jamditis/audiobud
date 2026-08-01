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

    /// On Windows, download and apply an available signed update, then exit
    #[arg(long)]
    pub install_update: bool,

    /// Override the signed update endpoint for release verification
    #[arg(long, requires = "install_update", hide = true)]
    pub install_update_endpoint: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::CliArgs;
    use clap::{CommandFactory, Parser};

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

    #[test]
    fn install_update_flag_is_available_for_release_verification() {
        let args = CliArgs::try_parse_from(["audiobud", "--install-update"])
            .expect("--install-update is accepted");
        assert!(args.install_update);
    }

    #[test]
    fn candidate_endpoint_requires_the_release_verification_flag() {
        let endpoint =
            "https://github.com/jamditis/audiobud/releases/download/v0.4.2/latest-candidate.json";
        let args = CliArgs::try_parse_from([
            "audiobud",
            "--install-update",
            "--install-update-endpoint",
            endpoint,
        ])
        .expect("release verification endpoint is accepted with --install-update");
        assert_eq!(args.install_update_endpoint.as_deref(), Some(endpoint));

        assert!(
            CliArgs::try_parse_from(["audiobud", "--install-update-endpoint", endpoint,]).is_err()
        );
    }
}
