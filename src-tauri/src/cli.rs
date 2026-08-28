use std::fmt;

const HELP: &str = "AudioBud - Speech to Text

Usage: audiobud [OPTIONS]

Options:
      --start-hidden          Start with the main window hidden
      --no-tray               Disable the system tray icon
      --toggle-transcription  Toggle transcription on/off (sent to running instance)
      --toggle-post-process   Toggle transcription with post-processing on/off (sent to running instance)
      --toggle-raw            Toggle raw transcription on/off (sent to running instance)
      --cancel                Cancel the current operation (sent to running instance)
      --debug                 Enable debug mode with verbose logging
      --install-update        On Windows, download and apply an available signed update, then exit
  -h, --help                  Print help
  -V, --version               Print version
";

/// The fixed command-line interface accepted by AudioBud.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CliArgs {
    pub start_hidden: bool,
    pub no_tray: bool,
    pub toggle_transcription: bool,
    pub toggle_post_process: bool,
    pub toggle_raw: bool,
    pub cancel: bool,
    pub debug: bool,
    pub install_update: bool,
    pub install_update_endpoint: Option<String>,
}

/// A successful parse can either run the app or print an informational response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliParseOutcome {
    Run(CliArgs),
    Help,
    Version,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliParseError {
    message: String,
}

impl CliParseError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for CliParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "error: {}\n\nFor more information, try '--help'.",
            self.message
        )
    }
}

impl std::error::Error for CliParseError {}

impl CliArgs {
    pub fn parse_env() -> Result<CliParseOutcome, CliParseError> {
        Self::parse_os_from(std::env::args_os())
    }

    /// Parse OS-native argv without decoding the executable path. Unix permits
    /// arbitrary bytes in that path, and startup must not panic merely because
    /// argv[0] is not UTF-8 (#285 review).
    fn parse_os_from<I, S>(arguments: I) -> Result<CliParseOutcome, CliParseError>
    where
        I: IntoIterator<Item = S>,
        S: Into<std::ffi::OsString>,
    {
        let mut arguments = arguments.into_iter();
        let _executable = arguments.next();
        let arguments = arguments
            .map(|argument| {
                argument.into().into_string().map_err(|invalid| {
                    CliParseError::new(format!("argument is not valid UTF-8: {invalid:?}"))
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Self::parse_options(arguments)
    }

    /// Parse an argv sequence. The first item is the executable name, matching
    /// both `std::env::args()` and the vector supplied by the single-instance
    /// plug-in.
    pub fn parse_from<I, S>(arguments: I) -> Result<CliParseOutcome, CliParseError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut arguments = arguments.into_iter().map(Into::into);
        let _executable = arguments.next();
        Self::parse_options(arguments)
    }

    fn parse_options<I>(arguments: I) -> Result<CliParseOutcome, CliParseError>
    where
        I: IntoIterator<Item = String>,
    {
        let mut arguments = arguments.into_iter();
        let mut parsed = Self::default();

        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "-h" | "--help" => return Ok(CliParseOutcome::Help),
                "-V" | "--version" => return Ok(CliParseOutcome::Version),
                "--start-hidden" => parsed.start_hidden = true,
                "--no-tray" => parsed.no_tray = true,
                "--toggle-transcription" => parsed.toggle_transcription = true,
                "--toggle-post-process" => parsed.toggle_post_process = true,
                "--toggle-raw" => parsed.toggle_raw = true,
                "--cancel" => parsed.cancel = true,
                "--debug" => parsed.debug = true,
                "--install-update" => parsed.install_update = true,
                "--install-update-endpoint" => {
                    let value = arguments.next().ok_or_else(|| {
                        CliParseError::new(
                            "a value is required for '--install-update-endpoint <VALUE>'",
                        )
                    })?;
                    if value.starts_with('-') {
                        return Err(CliParseError::new(
                            "a value is required for '--install-update-endpoint <VALUE>'",
                        ));
                    }
                    parsed.install_update_endpoint = Some(value);
                }
                _ => {
                    if let Some(value) = argument.strip_prefix("--install-update-endpoint=") {
                        if value.is_empty() {
                            return Err(CliParseError::new(
                                "a value is required for '--install-update-endpoint <VALUE>'",
                            ));
                        }
                        parsed.install_update_endpoint = Some(value.to_string());
                    } else if argument.starts_with('-') {
                        return Err(CliParseError::new(format!(
                            "unexpected option '{argument}'"
                        )));
                    } else {
                        return Err(CliParseError::new(format!(
                            "unexpected positional argument '{argument}'"
                        )));
                    }
                }
            }
        }

        if parsed.install_update_endpoint.is_some() && !parsed.install_update {
            return Err(CliParseError::new(
                "'--install-update-endpoint <VALUE>' requires '--install-update'",
            ));
        }

        Ok(CliParseOutcome::Run(parsed))
    }

    pub fn help() -> &'static str {
        HELP
    }

    pub fn version() -> String {
        format!("audiobud {}", env!("CARGO_PKG_VERSION"))
    }
}

#[cfg(test)]
mod tests {
    use super::{CliArgs, CliParseOutcome};

    fn run(arguments: &[&str]) -> CliArgs {
        match CliArgs::parse_from(arguments.iter().copied()).expect("arguments are valid") {
            CliParseOutcome::Run(arguments) => arguments,
            other => panic!("expected runnable arguments, got {other:?}"),
        }
    }

    #[test]
    fn every_boolean_option_is_supported() {
        assert!(run(&["audiobud", "--start-hidden"]).start_hidden);
        assert!(run(&["audiobud", "--no-tray"]).no_tray);
        assert!(run(&["audiobud", "--toggle-transcription"]).toggle_transcription);
        assert!(run(&["audiobud", "--toggle-post-process"]).toggle_post_process);
        assert!(run(&["audiobud", "--toggle-raw"]).toggle_raw);
        assert!(run(&["audiobud", "--cancel"]).cancel);
        assert!(run(&["audiobud", "--debug"]).debug);
        assert!(run(&["audiobud", "--install-update"]).install_update);
    }

    #[test]
    fn help_and_version_stay_available() {
        assert_eq!(
            CliArgs::parse_from(["audiobud", "--help"]),
            Ok(CliParseOutcome::Help)
        );
        assert_eq!(
            CliArgs::parse_from(["audiobud", "-h"]),
            Ok(CliParseOutcome::Help)
        );
        assert_eq!(
            CliArgs::parse_from(["audiobud", "--version"]),
            Ok(CliParseOutcome::Version)
        );
        assert_eq!(
            CliArgs::parse_from(["audiobud", "-V"]),
            Ok(CliParseOutcome::Version)
        );
        assert!(CliArgs::help().contains("AudioBud"));
        assert!(CliArgs::help().contains("On Windows, download"));
        assert!(CliArgs::version().starts_with("audiobud "));
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_executable_paths_are_discarded_before_decoding() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        assert_eq!(
            CliArgs::parse_os_from([
                OsString::from_vec(vec![0xff]),
                OsString::from("--start-hidden"),
            ]),
            Ok(CliParseOutcome::Run(CliArgs {
                start_hidden: true,
                ..CliArgs::default()
            }))
        );
    }

    #[test]
    fn endpoint_accepts_separated_and_joined_values() {
        let endpoint = "https://example.test/latest.json";
        for arguments in [
            vec![
                "audiobud".to_string(),
                "--install-update".to_string(),
                "--install-update-endpoint".to_string(),
                endpoint.to_string(),
            ],
            vec![
                "audiobud".to_string(),
                "--install-update".to_string(),
                format!("--install-update-endpoint={endpoint}"),
            ],
        ] {
            assert_eq!(
                match CliArgs::parse_from(arguments).unwrap() {
                    CliParseOutcome::Run(arguments) => arguments.install_update_endpoint,
                    other => panic!("expected runnable arguments, got {other:?}"),
                },
                Some(endpoint.to_string())
            );
        }
    }

    #[test]
    fn invalid_forms_are_rejected() {
        let cases = [
            vec!["audiobud", "--unknown"],
            vec!["audiobud", "unexpected"],
            vec!["audiobud", "--install-update-endpoint"],
            vec!["audiobud", "--install-update-endpoint="],
            vec![
                "audiobud",
                "--install-update-endpoint",
                "https://example.test/latest.json",
            ],
            vec!["audiobud", "--install-update-endpoint", "--install-update"],
        ];

        for arguments in cases {
            assert!(
                CliArgs::parse_from(arguments).is_err(),
                "accepted invalid arguments"
            );
        }
    }
}
