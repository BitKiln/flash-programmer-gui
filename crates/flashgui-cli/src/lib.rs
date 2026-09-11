pub mod cli;
pub mod commands;
pub mod exit_codes;
pub mod output;
pub mod profile;

use std::ffi::OsString;
use std::io::Write;

use clap::Parser;

pub use cli::{
    parse_address, Cli, Commands, EraseArgs, FlashArgs, ProfileSubcommand, Protocol, ResetArgs,
    VerifyArgs,
};
pub use exit_codes::{
    CliError, EXIT_FIRMWARE_PARSE_ERROR, EXIT_FLASH_VERIFY_ERROR, EXIT_INVALID_ARGS_OR_PROFILE,
    EXIT_PROBE_NOT_FOUND, EXIT_SUCCESS, EXIT_TARGET_CONNECTION_ERROR,
};
pub use profile::{
    delete_profile, list_profiles, load_profile, resolve_profile_path, save_profile, FlashProfile,
    ProfileSummary,
};

/// Runs the CLI with custom arguments and IO streams, returning the exit code.
pub fn run_cli_with_io<I, T>(
    args: I,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let cli = match Cli::try_parse_from(args) {
        Ok(c) => c,
        Err(err) => {
            if err.use_stderr() {
                let _ = write!(stderr, "{}", err);
                return EXIT_INVALID_ARGS_OR_PROFILE;
            } else {
                let _ = write!(stdout, "{}", err);
                return EXIT_SUCCESS;
            }
        }
    };

    let result = match &cli.command {
        Commands::Devices => commands::devices::handle_devices(&cli, stdout, stderr),
        Commands::Flash(ref flash_args) => {
            commands::flash::handle_flash(&cli, flash_args, stdout, stderr)
        }
        Commands::Erase(ref erase_args) => {
            commands::erase::handle_erase(&cli, erase_args, stdout, stderr)
        }
        Commands::Verify(ref verify_args) => {
            commands::verify::handle_verify(&cli, verify_args, stdout, stderr)
        }
        Commands::Reset(ref reset_args) => {
            commands::reset::handle_reset(&cli, reset_args, stdout, stderr)
        }
        Commands::Profile { ref action } => {
            commands::profile::handle_profile(&cli, action, stdout, stderr)
        }
    };

    match result {
        Ok(()) => EXIT_SUCCESS,
        Err(err) => {
            if !cli.json {
                let _ = writeln!(stderr, "{}", err);
            }
            err.exit_code()
        }
    }
}

/// Main entry point executing against standard process environment.
pub fn run() -> i32 {
    let mut stdout = std::io::stdout();
    let mut stderr = std::io::stderr();
    run_cli_with_io(std::env::args_os(), &mut stdout, &mut stderr)
}
