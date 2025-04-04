use std::{net::IpAddr, path::PathBuf};

use clap::Parser;
use figment::{
    providers::{Env, Format, Serialized, Toml},
    Figment,
};
use serde::{Deserialize, Serialize};

use crate::{auth::oidc::OidcConfig, logging::Verbosity};

#[derive(Deserialize, Debug)]
pub struct Config {
    #[serde(default = "defaults::listen_addr")]
    pub listen_addr: IpAddr,

    #[serde(default = "defaults::port")]
    pub port: u16,

    #[serde(default = "defaults::verbosity")]
    pub verbosity: Verbosity,

    #[serde(default = "defaults::log_file")]
    pub log_file: PathBuf,

    // no default! must be specified in some way
    pub db_url: String,
    pub secret_key: String,
    pub oidc_issuer_url: String,
    pub oidc_client_id: String,
    pub oidc_client_secret: String,
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

    pub fn get_rocket_config(&self) -> rocket::Config {
        let secret_key =
            hex::decode(&self.secret_key).expect("Fatal error: secret key is invalid hex sequence");

        if secret_key.len() != 64 {
            panic!(
                "Fatal error: secret key has incorrect length. Use, e.g., `openssl rand -hex 64` \
                 to generate"
            )
        }

        let ident = rocket::config::Ident::try_new("hive").unwrap();

        rocket::Config {
            address: self.listen_addr,
            port: self.port,
            secret_key: rocket::config::SecretKey::from(&secret_key),
            ident, // HTTP `Server` header
            ..Default::default()
        }
    }

    pub fn get_oidc_config(&self) -> OidcConfig {
        OidcConfig {
            issuer_url: self.oidc_issuer_url.clone(),
            client_id: self.oidc_client_id.clone(),
            client_secret: self.oidc_client_secret.clone(),
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
#[command(version, about, long_about = None)]
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

    /// Random 64-byte hex string (length 128) to use as secret key [no default]
    #[arg(short = 'k', long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret_key: Option<String>,

    /// OIDC server's issuer URL to use for authentication [no default]
    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oidc_issuer_url: Option<String>,

    /// OIDC client ID to use for authentication [no default]
    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oidc_client_id: Option<String>,

    /// OIDC client secret to use for authentication [no default]
    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oidc_client_secret: Option<String>,

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
