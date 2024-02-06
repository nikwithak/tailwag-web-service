use axum::{async_trait, Router};
use serde::Deserialize;
use tailwag_orm::{data_manager::PostgresDataProvider, queries::Insertable};

pub trait BuildRoutes<T: Insertable> {
    fn build_routes(data_manager: PostgresDataProvider<T>) -> Router;
}

pub trait BuildCreateRoute<'a>
where
    Self: Sized + Insertable,
{
    type Request: Into<Self> + Deserialize<'a>;
    fn build_create_route() -> Router;
}

pub trait BuildGetItemRoute<'a>
where
    Self: Sized,
{
    type Request: Into<Self> + Deserialize<'a>;
    fn build_get_item_route() -> Router;
}

pub trait BuildListGetRoute
where
    Self: Sized,
{
    fn build_list_get_route() -> Router;
}

// pub async fn form() -> Html<String> {
//     Html(
//         "
//         <form method=\"POST\" encType=\"application/json\" >
//             <label for=\"name\">Name</label>
//             <input type=\"text\" name=\"name\" />
//             <br />
//             <label for=\"style\">Style</label>
//             <input type=\"text\" name=\"style\" />
//             <input type=\"checkbox\" name=\"is_open_late\" value=\"true\" />
//             <br />
//             <button formaction=\"/food_truck\" type=\"submit\">Submit</button>
//         </form>
//         "
//         .into(),
//     )
// }
