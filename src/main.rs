use log::*;

mod config;
mod dto;
mod errors;
mod logging;
mod routing;
mod web;

#[rocket::launch]
async fn rocket() -> _ {
    let config = config::Config::get();

    logging::init_logger(config.verbosity, &config.log_file).expect("Failed to initialize logging");

    debug!("{config:?}");

    rocket::custom(config.get_rocket_config()).mount("/", &web::tree())
}
