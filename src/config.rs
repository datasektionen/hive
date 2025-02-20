use std::{net::IpAddr, path::PathBuf};

use clap::Parser;
use figment::{
    providers::{Env, Format, Serialized, Toml},
    Figment,
};
use serde::{Deserialize, Serialize};

use crate::logging::Verbosity;

#[derive(Deserialize, Debug)]
pub struct Config {
    #[serde(default = "defaults::listen_addr")]
    pub listen_addr: IpAddr,

    #[serde(default = "defaults::port")]
    pub port: u16,

    // no default! must be specified in some way
    pub db_url: String,

    #[serde(default = "defaults::verbosity")]
    pub verbosity: Verbosity,

    #[serde(default = "defaults::log_file")]
    pub log_file: PathBuf,
}

impl Config {
    pub fn get() -> Self {
        let args = CliArgs::parse();

        // merge semantic: bottom overrides top
        let result = Figment::new()
            .merge(Toml::file("hive.toml"))
            .merge(Env::prefixed("HIVE_"))
            .merge(Serialized::defaults(args)) // CLI
            .extract();

        match result {
            Ok(config) => config,
            Err(errors) => {
                for error in errors {
                    eprintln!("Fatal configuration error: {error}");
                }
                panic!("Failed to determine a valid configuration")
            }
        }
    }
}

// sadly must be a separate struct from Config because otherwise
// it would force db_url to always be set through cli, since
// parse would fail with `String` and would always override to
// `None` if `Option<String>`; this is also what the serde annotation
// prevents -- unfortunately we cannot specify default values through
// clap since we only want to override configs if the user explicitly
// requests it
#[derive(Parser, Serialize, Deserialize, Debug)]
pub struct CliArgs {
    /// IP address to listen for connections on [default: 0.0.0.0]
    #[arg(short, long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub listen_addr: Option<IpAddr>,

    /// Port to listen to connections on [default: 6869]
    #[arg(short, long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,

    /// Database PostgreSQL connection string to use [no default]
    #[arg(short, long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub db_url: Option<String>,

    /// How much information to show and log [default: normal]
    #[arg(short, long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verbosity: Option<Verbosity>,

    /// File to log to, in append mode [default: /tmp/hive.log]
    #[arg(short = 'f', long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_file: Option<PathBuf>,
}

// unfortunately #[serde(default = "path")] only allows specifying
// functions and not values directly, so these const fns must exist;
// we cannot impl Default because db_url cannot have a default
mod defaults {
    use std::{
        net::{IpAddr, Ipv4Addr},
        path::PathBuf,
    };

    use crate::logging::Verbosity;

    pub const fn listen_addr() -> IpAddr {
        IpAddr::V4(Ipv4Addr::UNSPECIFIED) // 0.0.0.0
    }

    pub const fn port() -> u16 {
        6869
    }

    pub const fn verbosity() -> Verbosity {
        Verbosity::Normal
    }

    pub fn log_file() -> PathBuf {
        PathBuf::from("/tmp/hive.log")
    }
}
