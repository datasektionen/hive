pub mod api_tokens;
pub mod audit_logs;
pub mod groups;
pub mod permissions;
pub mod systems;

macro_rules! pg_args {
    ($($arg:expr),+) => {
        {
            use sqlx::{Arguments, postgres::PgArguments};
            use crate::errors::AppError;

            let mut args = PgArguments::default();

            $(
                args.add($arg).map_err(AppError::QueryBuildError)?;
            )*

            args
        }
    };
}

// required to allow the `allow()` below
#[allow(clippy::useless_attribute)]
// required for usage in this module's children
#[allow(clippy::needless_pub_self)]
pub(self) use pg_args;
