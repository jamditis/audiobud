/// Returns the appropriate CPAL host for the current platform.
/// On Linux, uses ALSA host. On other platforms, uses the default host.
pub fn get_cpal_host() -> cpal::Host {
    cpal::default_host()
}
