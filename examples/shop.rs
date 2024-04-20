use tailwag_macros::derive_magic;
use tailwag_web_service::auth::gateway;
use uuid::Uuid;

// Needed to simulate  the consolidation library that doesn't actually exist in this scope.
// TODO: Fix this bloody thing, it's annoying.
mod tailwag {
    pub use tailwag_forms as forms;
    pub use tailwag_macros as macros;
    pub use tailwag_orm as orm;
    pub use tailwag_web_service as web;
}

derive_magic! {
    pub struct Product {
        id: Uuid,
        name: String,
        description: String,
    }
}

// TODO: Wrap all of these in a single "resources" macro, that splits off of "struct"
derive_magic! {
    pub struct ShopOrder {
        id: Uuid,
        customer_name: String,
        customer_email: String,
        status: String,
        stripe_order_id: String,
        // name: String,
        // description: String,
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct CartItem {
    id: Uuid,
}

pub mod checkout {
    use tailwag_orm::{
        data_definition::exp_data_system::DataSystem,
        data_manager::{traits::DataProvider, PostgresDataProvider},
    };
    use tailwag_web_service::application::http::route::{IntoResponse, Response};

    use crate::{CartItem, Product, ShopOrder};

    #[derive(serde::Serialize, serde::Deserialize, Debug)]
    pub struct CheckoutRequest {
        cart_items: Vec<CartItem>,
        // customer_name:  String,
        // customer_email: String,
    }
    pub async fn checkout(
        req: CheckoutRequest,
        providers: DataSystem,
    ) -> Response {
        let Some(products) = providers.get::<Product>() else {
            log::error!("No Products data provider was found");
            return Response::internal_server_error();
        };
        let Some(orders) = providers.get::<ShopOrder>() else {
            log::error!("No Orders data provider was found");
            return Response::internal_server_error();
        };
        let order =
            <PostgresDataProvider<ShopOrder> as DataProvider<ShopOrder>>::CreateRequest::default();
        let order = orders.create(order).await.unwrap();
        // let Ok(order) = orders.create(order).await else {
        //     log::error!("Failed to create order");
        //     // TODO: Figure out how to consume the ? operator here. Writing this every time is annoying.
        //     return Response::internal_server_error();
        // };

        // TODO: Call Stripe
        {}

        println!("Got a request: {:?}", req);
        Response::redirect_see_other("http://localhost:3000")
    }
}

#[tokio::main]
async fn main() {
    tailwag_web_service::application::WebService::builder("My Events Service")
        // .with_before(gateway::AuthorizationGateway)
        .post_public("/login", gateway::login)
        .post_public("/register", gateway::register)
        .with_resource::<Product>() // TODO- public GET  owith filtering)
        .with_resource::<ShopOrder>() // TODO - public POST, restricted GET (to specific customer, via email)
        // .with_resource::<Customer>() // TODO - No public - (nly created from stripe stuff)
        //     .service(webhooks::stripe::stripe_webhook)
        .get_public("/checkout", checkout::checkout) // TODO
        .build_service()
        .run()
        .await
        .unwrap();
}
