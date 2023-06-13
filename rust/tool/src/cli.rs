use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, Args};

use crate::install;
use crate::signature::KeyPair;

/// The default log level.
///
/// 2 corresponds to the level INFO.
const DEFAULT_LOG_LEVEL: usize = 2;

#[derive(Parser)]
pub struct Cli {
    /// Silence all output
    #[arg(short, long)]
    quiet: bool,
    /// Verbose mode (-v, -vv, etc.)
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,
    #[clap(subcommand)]
    commands: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Install(InstallArgs),
}

#[derive(Debug, Args)]
struct InstallArgs {
    #[command(subcommand)]
    command: BootloaderCommand,

    #[command(flatten)]
    generic_args: GenericBootloaderArgs,
}

#[derive(Debug, Args)]
#[command(args_conflicts_with_subcommands = false)]
struct GenericBootloaderArgs {
    /// sbsign Public Key
    #[arg(long)]
    public_key: PathBuf,

    /// sbsign Private Key
    #[arg(long)]
    private_key: PathBuf,

    /// Configuration limit
    #[arg(long, default_value_t = 1)]
    configuration_limit: usize,

    /// EFI system partition mountpoint (e.g. efiSysMountPoint)
    esp: PathBuf,

    /// List of generation links (e.g. /nix/var/nix/profiles/system-*-link)
    generations: Vec<PathBuf>,
}

#[derive(Debug, Subcommand)]
enum BootloaderCommand {
    SystemdBoot(SdBootArgs),
    UBoot(UBootArgs),
}

#[derive(Debug, Parser)]
struct UBootArgs {
    /// Systemd path
    #[arg(long)]
    systemd: PathBuf,

    /// Systemd-boot loader config
    #[arg(long)]
    systemd_boot_loader_config: PathBuf,
}

#[derive(Debug, Parser)]
struct SdBootArgs {
    /// Systemd path
    #[arg(long)]
    systemd: PathBuf,

    /// Systemd-boot loader config
    #[arg(long)]
    systemd_boot_loader_config: PathBuf,
}

impl Cli {
    pub fn call(self, module: &str) {
        stderrlog::new()
            .module(module)
            .show_level(false)
            .quiet(self.quiet)
            .verbosity(DEFAULT_LOG_LEVEL + usize::from(self.verbose))
            .init()
            .expect("Failed to setup logger.");

        if let Err(e) = self.commands.call() {
            log::error!("{e:#}");
            std::process::exit(1);
        };
    }
}

impl Commands {
    pub fn call(self) -> Result<()> {
        match self {
            Commands::Install(args) => install(args),
        }
    }
}

fn install(args: InstallArgs) -> Result<()> {
    let lanzaboote_stub =
        std::env::var("LANZABOOTE_STUB").context("Failed to read LANZABOOTE_STUB env variable")?;

    let key_pair = KeyPair::new(&args.generic_args.public_key, &args.generic_args.private_key);

    match args.command {
        BootloaderCommand::SystemdBoot(sd_boot_args) => install::Installer::new(
            PathBuf::from(lanzaboote_stub),
            sd_boot_args.systemd,
            sd_boot_args.systemd_boot_loader_config,
            key_pair,
            args.generic_args.configuration_limit,
            args.generic_args.esp,
            args.generic_args.generations,
        )
        .install(),
        _ => todo!()
    }
}
