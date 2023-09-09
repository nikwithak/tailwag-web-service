struct RoutePath {
    path: String,
}

pub struct Route {
    path: String,
    method: HttpMethod,
    function: Fn,
}
