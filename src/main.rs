//
//! # Minimal implementation of Redshift in Rust
//!
//! aka redshift-minimal-rs
//!

#[macro_use]
extern crate lazy_static;

// Optional features for gamma method providers
#[cfg(feature = "randr")]
extern crate xcb;

mod colorramp;
mod gamma;
mod transition;
use transition::ColorSetting;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const ABOUT: &str = "
Set color temperature of display based on argument.

A Stripped down Rust clone of the original Redshift written in C by Jon Lund Steffensen.";
const USAGE: &str = r#"
USAGE:
    redshift-minimal-rs [OPTIONS]
    redshift-minimal-rs (-h | --help)
    redshift-minimal-rs (-V | --version)
"#;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

// Constants
const NEUTRAL_TEMP: i32 = 6500;
const MIN_TEMP: i32 = 1000;
const MAX_TEMP: i32 = 25000;

fn usage() {
    println!("redshift-minimal-rs {VERSION}");
    println!("{ABOUT}");
    println!("{USAGE}");
    println!(
        r#"OPTIONS:
    -S, --Set <TEMP>      (set color temperature)
"#
    );
}

/// Selected run mode
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
enum Mode {
    /// Reset the screen
    Reset,

    /// One shot manual mode - set color temperature
    Manual(i32),
}

impl Mode {
    fn as_args(&self) -> &str {
        match self {
            Mode::Reset => "--reset|-x",
            Mode::Manual(..) => "--Set|-S",
        }
    }
}

struct Args {
    pub help: bool,
    pub version: bool,
    pub method: Option<String>,
    pub mode: Mode,
}

impl Args {
    pub fn defaults() -> Args {
        Args {
            help: false,
            version: false,
            method: None,
            mode: Mode::Manual(NEUTRAL_TEMP),
        }
    }

    /// Parse the command-line arguments into a Redshift configuration
    pub fn update_from_args(mut self) -> Result<Args> {
        let args = std::env::args().skip(1).collect::<Vec<_>>();

        if args[0] == "-h" || args[0] == "--help" {
            self.help = true;
            // A short-cut: We should just print the usage and exit, so no need
            // to run any subsequent checks.
            return Ok(self);
        }

        if args[0] == "-V" || args[0] == "--version" {
            self.version = true;
            return Ok(self);
        }

        // Detect the mode
        // All four are mutually excluse (at most one of them may be present)
        let mut mode: Option<Mode> = None;

        if args[0] == "-S" || args[0] == "--Set" {
            if args.len() < 2 {
                return Err("Missing argument for -S".into());
            }
            let t = args[1].parse::<i32>().unwrap();

            if !(MIN_TEMP..=MAX_TEMP).contains(&t) {
                return malformed(format!(
                    "Temperature must be between {MIN_TEMP} and {MAX_TEMP} (was {t})",
                ));
            }
            mode = Some(Mode::Manual(t));
        }

        if args[0] == "-x" || args[0] == "--reset" {
            if let Some(m) = mode {
                return malformed(format!(
                    "Mode '{}' cannot be used in conjuction with '{}'",
                    Mode::Reset.as_args(),
                    m.as_args()
                ));
            }
            mode = Some(Mode::Reset);
        }

        self.mode = mode.unwrap_or(self.mode);

        Ok(self)
    }
}

#[inline]
fn malformed<T>(msg: String) -> Result<T> {
    Err(msg.into())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::defaults().update_from_args()?;
    if args.help {
        usage();
        return Ok(());
    }

    if args.version {
        println!("redshift-minimal-rs {VERSION}");
        return Ok(());
    }

    match args.mode {
        Mode::Reset => {
            let mut gamma_state = gamma::init_gamma_method(args.method.as_deref())?;
            gamma_state.start()?;
            gamma_state.set_temperature(&ColorSetting {
                temp: NEUTRAL_TEMP,
                gamma: [1.0, 1.0, 1.0],
                brightness: 1.0,
            })?;
        }
        Mode::Manual(temp) => {
            let color_setting = ColorSetting {
                temp,
                gamma: [1.0, 1.0, 1.0],
                brightness: 1.0,
            };

            let mut gamma_state = gamma::init_gamma_method(args.method.as_deref())?;
            gamma_state.start()?;
            gamma_state.set_temperature(&color_setting)?;
        }
    }

    Ok(())
}
