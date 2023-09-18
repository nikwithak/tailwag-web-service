use axum::{async_trait, Router};
use serde::Deserialize;
use tailwag_orm::{
    data_manager::PostgresDataProvider,
    queries::{Insertable, Queryable},
};

#[async_trait]
pub trait BuildRoutes<T: Queryable + Insertable> {
    async fn build_routes(data_manager: PostgresDataProvider<T>) -> Router;
}

// #[async_trait]
// impl<'a, T> BuildRoutes<T> for T
// where
//     T: Queryable + Insertable + BuildCreateRoute<'a> + BuildListGetRoute + Send,
// {
//     async fn build_routes(data_manager: PostgresDataProvider<T>) -> Router {
//         Router::new()
//             .nest("/", Self::build_create_route())
//             .nest("/", Self::build_list_get_route())
//     }
// }

pub trait BuildCreateRoute<'a>
where
    Self: Sized + Insertable,
{
    // type Request: Into<Self> + Deserialize<'a>;
    fn build_create_route() -> Router;
}

pub trait BuildGetItemRoute<'a>
where
    Self: Sized + Queryable,
{
    type Request: Into<Self> + Deserialize<'a>;
    fn build_list_get_route() -> Router;
}

pub trait BuildListGetRoute
where
    Self: Sized + Queryable,
{
    fn build_list_get_route() -> Router;
}

// // TODO: macro for the function to wrap in `Query` or `Json` automatically, so it can just be a decoration on a logic function.
// pub async fn post(
//     State(data_manager): State<PostgresDataProvider<T>>,
//     axum:textract::Form(request): axum::extract::Form<CreateFoodTruckRequest>,
//) -> Json<FoodTruck> {
//     let truck = FoodTruck {
//         id: Uuid::new_v4(),
//         name: request.name.clone(),
//         style: request.style.clone(),
//         is_open_late: request.is_open_late.clone(),
//     };
//     data_manager.create(&truck).await.expect("Unable to create object");
//     Json(truck)
// }

// // TODO: macro for the function to wrap in `Query` or `Json` automatically, so it can just be a macro over a logic function.
// pub async fn get_food_trucks(
//     State(data_manager): State<FoodTruckDataManager>,
//     // Query(request): Query<GetFoodTrucksRequest>,
// ) -> Json<Vec<FoodTruck>> {
//     let q = data_manager.all();
//     let results = q.execute().await.unwrap();
//     Json(results)
// }

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
