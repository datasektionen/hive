use crate::routing::RouteTree;

mod groups;

pub fn tree() -> RouteTree {
    RouteTree::Branch(vec![groups::routes()])
}
