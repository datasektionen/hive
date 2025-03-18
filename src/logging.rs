use std::{io, path};

use clap::ValueEnum;
use log::*;
use serde::{Deserialize, Serialize};

#[derive(ValueEnum, Serialize, Deserialize, Debug, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum Verbosity {
    #[serde(alias = "off")]
    Quiet,
    #[serde(alias = "warn")]
    Normal,
    #[serde(alias = "info")]
    Verbose,
    #[serde(alias = "debug")]
    VeryVerbose,
}

impl From<Verbosity> for LevelFilter {
    fn from(verbosity: Verbosity) -> Self {
        match verbosity {
            Verbosity::Quiet => Self::Off,
            Verbosity::Normal => Self::Warn,
            Verbosity::Verbose => Self::Info,
            Verbosity::VeryVerbose => Self::Debug,
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum InitLoggerError {
    #[error("failed to open log file: {0}")]
    OpenLogFile(#[from] io::Error),
    #[error("failed to set logger (another logger has already been registered): {0}")]
    SetLog(#[from] log::SetLoggerError),
}

pub fn init_logger(verbosity: Verbosity, log_file: &path::Path) -> Result<(), InitLoggerError> {
    let level_filter: LevelFilter = verbosity.into();

    simplelog::CombinedLogger::init(vec![
        simplelog::TermLogger::new(
            level_filter,
            simplelog::Config::default(),
            simplelog::TerminalMode::Stderr,
            simplelog::ColorChoice::Auto,
        ),
        simplelog::WriteLogger::new(
            level_filter,
            simplelog::Config::default(),
            std::fs::File::options()
                .append(true)
                .create(true)
                .open(log_file)?,
        ),
    ])?;

    Ok(())
}
