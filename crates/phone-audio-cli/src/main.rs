//! phone-audio command-line interface.
//!
//! Wire the phone's audio into the PC: `phone-audio on`, `phone-audio off`,
//! `phone-audio status`, `phone-audio volume 80`, `phone-audio set-phone`.

use std::process::ExitCode;

use clap::{Parser, Subcommand};
use phone_audio_core::{App, Status};

#[derive(Parser)]
#[command(
    name = "phone-audio",
    version,
    about = "Route a Bluetooth phone's audio to this PC via PipeWire"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Show current status (phone, profile, loopback, volume)
    Status {
        /// Emit machine-readable JSON
        #[arg(long)]
        json: bool,
    },
    /// List connected bluetooth phones
    List {
        /// Emit machine-readable JSON
        #[arg(long)]
        json: bool,
    },
    /// Remember which phone to use (name or MAC)
    SetPhone { name_or_mac: String },
    /// Turn the phone's audio on (switch profile + start loopback)
    On,
    /// Turn it off (stop loopback, drop profile)
    Off,
    /// Toggle on/off
    Toggle,
    /// Set volume 0-100 percent
    Volume {
        #[arg(value_parser = clap::value_parser!(u8).range(0..=100))]
        percent: u8,
    },
    /// Multi-line troubleshooting dump
    Debug,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli.command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("phone-audio: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(cmd: Command) -> phone_audio_core::Result<()> {
    let mut app = App::new()?;
    match cmd {
        Command::Status { json } => {
            let s = app.status()?;
            if json {
                println!("{}", serde_json::to_string_pretty(&s)?);
            } else {
                print_status(&s);
            }
        }
        Command::List { json } => {
            let phones = app.list_phones()?;
            if json {
                println!("{}", serde_json::to_string_pretty(&phones)?);
            } else {
                for p in &phones {
                    println!("{}\t{}", p.name, p.mac);
                }
                if phones.is_empty() {
                    println!("(no bluetooth phones connected)");
                }
            }
        }
        Command::SetPhone { name_or_mac } => {
            let p = app.set_phone(&name_or_mac)?;
            println!("using {}", p);
        }
        Command::On => {
            app.turn_on()?;
            println!("Routing: on");
        }
        Command::Off => {
            app.turn_off()?;
            println!("Routing: off");
        }
        Command::Toggle => {
            let on = app.toggle()?;
            println!("{}", if on { "on" } else { "off" });
        }
        Command::Volume { percent } => app.set_volume(f64::from(percent))?,
        Command::Debug => print!("{}", app.debug()?),
    }
    Ok(())
}

fn print_status(s: &Status) {
    if !s.available {
        println!("no phone: {}", s.reason);
        return;
    }
    let phone = s
        .phone
        .as_ref()
        .map(|p| p.to_string())
        .unwrap_or_else(|| "?".into());
    println!("phone:   {phone}");
    match &s.profile {
        Some(p) => println!("profile: {p}"),
        None => println!("profile: -"),
    }
    println!("state:   {}", if s.on { "on" } else { "off" });
    match s.volume {
        Some(v) => println!("volume:  {v:.0}%"),
        None => println!("volume:  -"),
    }
    if !s.reason.is_empty() {
        println!("note:    {}", s.reason);
    }
}
