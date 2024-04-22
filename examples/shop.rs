use std::sync::Arc;

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
        stripe_price_id: String,
    }
}

// TODO (interface improvement): Wrap all of these in a single "resources" macro, that splits off of "struct"
derive_magic! {
    #[actions(stripe_event)]

    pub struct ShopOrder {
        id: Uuid,
        customer_name: String,
        customer_email: String,
        status: String,
        stripe_order_id: String,
        // line_items: Vec<String>, // TODO: Fix thiiiis
        // name: String,
        // description: String,
    }
}

pub async fn stripe_event(
    id: String,
    data_providers: tailwag_orm::data_definition::exp_data_system::DataSystem,
) -> Option<Vec<String>> {
    Some(vec!["HEllo".to_string()])
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct CartItem {
    id: Uuid,
    quantity: usize,
    // TODO:
    // customizations: Vec<Customization>,
}

pub mod checkout {
    use std::{collections::HashMap, sync::Arc};

    use stripe::{generated::core::product, *};
    use tailwag_orm::{
        data_definition::exp_data_system::DataSystem,
        data_manager::{traits::DataProvider, PostgresDataProvider},
        queries::filterable_types::FilterEq,
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

        let products_fut = req.cart_items.iter().map(|i| {
            products.get(
                move |filter| filter.id.eq(i.id), // .eq(i.product_id.clone())
            )
        });
        let mut products = Vec::new();
        for product in products_fut {
            products.push(product.await.unwrap().unwrap())
        }

        type OrderCreateRequest =
            <PostgresDataProvider<ShopOrder> as DataProvider<ShopOrder>>::CreateRequest;
        let order = OrderCreateRequest {
            id: uuid::Uuid::new_v4(),
            ..Default::default()
        };
        let order = orders.create(order).await.unwrap();
        // let Ok(order) = orders.create(order).await else {
        //     log::error!("Failed to create order");
        //     // TODO: Figure out how to consume the ? operator here. Writing this every time is annoying.
        //     return Response::internal_server_error();
        // };

        log::debug!("Got a request: {:?}", req);
        let Some(url) = create_stripe_session(order, products, &stripe_client).await.url else {
            return Response::internal_server_error();
        };

        // Response::redirect_see_other(&url)
        vec![("payment_url", url)]
            .into_iter()
            .collect::<HashMap<&str, String>>()
            .into_response()
    }

    async fn create_stripe_session(
        order: ShopOrder,
        products: Vec<Product>,
        stripe_client: &stripe::Client,
    ) -> stripe::CheckoutSession {
        let order_id = &order.id.to_string();
        let success_url = format!("http://localhost:3000/order/{}", &order.id);
        let params = stripe::CreateCheckoutSession {
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
                products
                    .iter()
                    .map(|li| CreateCheckoutSessionLineItems {
                        adjustable_quantity: None,
                        dynamic_tax_rates: None,
                        price: Some(li.stripe_price_id.clone()),
                        price_data: None,
                        quantity: Some(1), // TODO: Actually get the quantity.
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
        .with_resource::<Product>() // TODO- public GET with filtering)
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
