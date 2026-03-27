use sqlx::QueryBuilder;

use crate::{
    dto::datetime::BrowserDateTimeDto, models::{ActionKind, TargetKind}, sanitizers::SearchTerm
};

#[derive(Debug)]
pub struct LogsFilterDto<'r> {
    pub actor: Option<&'r str>,
    pub action: Option<ActionKind>,
    pub target: Option<TargetKind>,
    pub id: Option<&'r str>,
    pub from: Option<BrowserDateTimeDto>,
    pub until: Option<BrowserDateTimeDto>,
    pub order: bool
}

impl LogsFilterDto<'_> {
    fn any(&self) -> bool {
        self.actor.is_some()
            || self.action.is_some()
            || self.target.is_some()
            || self.id.is_some()
            || self.from.is_some()
            || self.until.is_some()
    }

    pub fn query<'a>(&self, query: &mut QueryBuilder<'a, sqlx::Postgres>) {
        let mut added = false;
        if self.any() {
            query.push(" WHERE");
        }


        if let Some(action) = &self.action {
            query.push(" action_kind = ");
            query.push_bind(action.clone());
            added = true;
        }

        if let Some(from) = &self.from {
            if added {
                query.push(" AND");
            }

            query.push(" stamp >= ");
            query.push_bind(from.clone());
            added = true;
        }

        if let Some(until) = &self.until {
            if added {
                query.push(" AND");
            }

            query.push(" stamp <= ");
            query.push_bind(until.clone());
            added = true;
        }

        if let Some(target) = &self.target {
            if added {
                query.push(" AND");
            }
            query.push(" target_kind = ");
            query.push_bind(target.clone());
            added = true;
        }

        if let Some(actor) = self.actor {
            let term = SearchTerm::from(actor).anywhere();
            if added {
                query.push(" AND");
            }
            query.push(" actor LIKE ");
            query.push_bind(term);
            added = true;
        }

        if let Some(id) = self.id {
            let term = SearchTerm::from(id).anywhere();
            if added {
                query.push(" AND");
            }
            query.push(" target_id LIKE ");
            query.push_bind(term);
        }
    }
}
