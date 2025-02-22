use log::*;
use sqlx::PgPool;

mod config;
mod dto;
mod errors;
mod guards;
mod logging;
mod models;
mod routing;
mod web;

#[rocket::launch]
async fn rocket() -> _ {
    let config = config::Config::get();

    logging::init_logger(config.verbosity, &config.log_file).expect("Failed to initialize logging");

    debug!("{config:?}");

    let db = PgPool::connect(&config.db_url)
        .await
        .expect("Failed to connect to the database");

    debug!("Initialized database connection pool");

    sqlx::migrate!("./migrations")
        .run(&db)
        .await
        .expect("Failed to apply database migrations");

    info!("Database migrations successfully applied");

    rocket::custom(config.get_rocket_config())
        .manage(db)
        .mount("/", &web::tree())
}
