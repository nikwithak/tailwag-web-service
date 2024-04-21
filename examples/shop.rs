use std::sync::Arc;

use tailwag_macros::derive_magic;
use tailwag_web_service::{application::http::route::IntoResponse, auth::gateway};
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
        // line_items: Vec<String>,
        // name: String,
        // description: String,
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct CartItem {
    id: Uuid,
}

pub mod checkout {
    use std::sync::Arc;

    use stripe::*;
    use tailwag_orm::{
        data_definition::exp_data_system::DataSystem,
        data_manager::{traits::DataProvider, PostgresDataProvider},
    };
    use tailwag_web_service::application::http::route::{IntoResponse, Response, ServerData};

    use crate::{CartItem, Product, ShopOrder};

    #[derive(serde::Serialize, serde::Deserialize, Debug)]
    pub struct CheckoutRequest {
        cart_items: Vec<CartItem>,
        customer_name: Option<String>,
        customer_email: Option<String>,
    }
    pub async fn checkout(
        req: CheckoutRequest,
        providers: DataSystem,
        stripe_client: ServerData<stripe::Client>,
    ) -> Response {
        let Some(products) = providers.get::<Product>() else {
            log::error!("No Products data provider was found");
            return Response::internal_server_error();
        };
        let Some(orders) = providers.get::<ShopOrder>() else {
            log::error!("No Orders data provider was found");
            return Response::internal_server_error();
        };
        let mut order =
            <PostgresDataProvider<ShopOrder> as DataProvider<ShopOrder>>::CreateRequest::default();
        order.id = uuid::Uuid::new_v4();
        let order = orders.create(order).await.unwrap();
        // let Ok(order) = orders.create(order).await else {
        //     log::error!("Failed to create order");
        //     // TODO: Figure out how to consume the ? operator here. Writing this every time is annoying.
        //     return Response::internal_server_error();
        // };

        log::debug!("Got a request: {:?}", req);
        let Some(url) = create_stripe_session(order, &stripe_client).await.url else {
            return Response::internal_server_error();
        };

        Response::redirect_see_other(&url)
    }

    async fn create_stripe_session(
        order: ShopOrder,
        stripe_client: &stripe::Client,
    ) -> stripe::CheckoutSession {
        let order_id = &order.id.to_string();
        let success_url = format!("http://localhost:3000/order/{}", &order.id);
        let mut params = stripe::CreateCheckoutSession {
            success_url: Some(&success_url), // TODO: Configure this
            // customer_email: Some(&order.customer_email),
            shipping_address_collection: Some(
                stripe::CreateCheckoutSessionShippingAddressCollection {
                    allowed_countries: vec![
                        CreateCheckoutSessionShippingAddressCollectionAllowedCountries::Us,
                    ],
                },
            ),
            automatic_tax: Some(CreateCheckoutSessionAutomaticTax {
                enabled: true,
                liability: None,
            }),
            payment_intent_data: Some(CreateCheckoutSessionPaymentIntentData {
                receipt_email: None,
                ..Default::default()
            }),
            client_reference_id: Some(order_id),
            mode: Some(stripe::CheckoutSessionMode::Payment),
            line_items: Some(
                vec!["Hi there"]
                    .iter()
                    .map(|li| CreateCheckoutSessionLineItems {
                        adjustable_quantity: None,
                        dynamic_tax_rates: None,
                        price: Some(
                            "price_1LLXcyF5uyr8Gny9YzCapbn9".to_owned(), // TODO: unhardcode
                                                                         // li.price.stripe_price_id.clone().expect("Expected stripe_price_id"),
                        ),
                        price_data: None,
                        quantity: Some(1),
                        tax_rates: None,
                    })
                    .collect(),
            ),
            ..Default::default()
        };

        CheckoutSession::create(stripe_client, params).await.unwrap()
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
        // .with_resource::<Customer>() // TODO - No public - (nly created from stripe stuff, for now)
        //     .service(webhooks::stripe::stripe_webhook)
        .post_public("/checkout", checkout::checkout) // TODO
        .with_server_data(stripe::Client::new(
            std::env::var("STRIPE_API_KEY").expect("STRIPE_API_KEY is missing from env."),
        ))
        .build_service()
        .run()
        .await
        .unwrap();
}
