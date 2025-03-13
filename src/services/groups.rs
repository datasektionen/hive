pub mod details;
pub mod list;

pub enum GroupMembershipKind {
    Indirect,
    Direct,
}

pub enum RoleInGroup {
    Member,
    Manager,
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub enum AuthorityInGroup {
    None,
    ManageMembers,
    FullyAuthorized,
}

pub struct GroupRelevance {
    pub role: Option<RoleInGroup>,
    pub authority: AuthorityInGroup,
}

impl GroupRelevance {
    pub fn new(role: Option<RoleInGroup>, authority: AuthorityInGroup) -> Option<Self> {
        if role.is_none() && authority == AuthorityInGroup::None {
            None
        } else {
            Some(Self { role, authority })
        }
    }
}
