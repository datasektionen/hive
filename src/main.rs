use errors::ErrorPageGenerator;
use log::*;
use rocket::fs::FileServer;
use sqlx::PgPool;

mod api;
mod config;
mod dto;
mod errors;
mod guards;
mod logging;
mod models;
mod perms;
mod routing;
mod sanitizers;
mod services;
mod web;

const HIVE_SYSTEM_ID: &str = "hive";
const HIVE_ROOT_GROUP_ID: &str = "root";
const HIVE_INTERNAL_DOMAIN: &str = "hive.internal";

rust_i18n::i18n!("./locales");

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

    info!(
        "Available i18n locales: {:?}",
        rust_i18n::available_locales!()
    );

    rocket::custom(config.get_rocket_config())
        .manage(db)
        .attach(ErrorPageGenerator)
        .mount("/", &web::tree())
        .mount("/api/v0", &api::v0::tree())
        .mount("/api/v1", &api::v1::tree())
        .mount("/static", FileServer::from("./static"))
        .register("/api", api::catchers())
        .register("/", web::catchers())
}
