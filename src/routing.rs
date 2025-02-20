use rocket::Route;

// convenient for a modular distribution of routes across files,
// without having to centralize a single list of all routes here
pub enum RouteTree {
    Leaf(Box<Route>),
    Branch(Vec<RouteTree>),
}

impl From<Vec<Route>> for RouteTree {
    fn from(vec: Vec<Route>) -> Self {
        Self::Branch(
            vec.iter()
                .map(|r| Self::Leaf(Box::new(r.clone())))
                .collect(),
        )
    }
}

impl From<&RouteTree> for Vec<Route> {
    fn from(tree: &RouteTree) -> Self {
        match tree {
            RouteTree::Leaf(route) => vec![*route.clone()],
            RouteTree::Branch(routes) => routes.iter().flat_map(Vec::from).collect(),
        }
    }
}
