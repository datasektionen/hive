use log::*;

mod config;
mod logging;

fn main() {
    let config = config::Config::get();

    logging::init_logger(config.verbosity, &config.log_file).expect("Failed to initialize logging");

    debug!("{config:?}");

    todo!()
}
