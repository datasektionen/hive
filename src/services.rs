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

macro_rules! update_if_changed {
    ($map:expr, $query:expr, $prop:ident, $old:expr, $new:expr) => {
        if $old.$prop != *$new.$prop {
            if !$map.is_empty() {
                $query.push(", ");
            }
            $query.push(format!(" {} = ", stringify!($prop)));
            $query.push_bind($new.$prop);

            $map.insert(
                stringify!($prop),
                (
                    serde_json::Value::from($old.$prop),
                    serde_json::Value::from($new.$prop),
                ),
            );
        };
    };
}

macro_rules! audit_log_details_for_update {
    ($map:expr) => {
        serde_json::json!({
            "old": $map.iter().map(|(k, (old, _))| ((*k).to_owned(), old.clone())).collect::<serde_json::Map<_, _>>(),
            "new": $map.into_iter().map(|(k, (_, new))| (k.to_owned(), new)).collect::<serde_json::Map<_, _>>(),
        })
    };
}

// required to allow the `allow()` below
#[allow(clippy::useless_attribute)]
// required for usage in this module's children
#[allow(clippy::needless_pub_self)]
pub(self) use {audit_log_details_for_update, pg_args, update_if_changed};
