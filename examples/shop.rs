use std::sync::Arc;

use serde::{Deserialize, Serialize};
use stripe::Event;
use tailwag_macros::derive_magic;
use tailwag_orm::data_manager::{traits::DataProvider, PostgresDataProvider};
use tailwag_web_service::{
    application::http::route::{FromRequest, IntoResponse, Request, Response, ServerData},
    auth::gateway,
    tasks::{
        runner::{TaskExecutor, TaskResult},
        TaskScheduler,
    },
};
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
    #[post(create_product)]
    pub struct Product {
        id: Uuid,
        name: String,
        description: String,
        stripe_price_id: String,
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct CreateProductRequest {
    name: String,
    description: Option<String>,
}

pub async fn create_product(
    req: CreateProductRequest,
    products: PostgresDataProvider<Product>,
) -> Product {
    let stripe_price_id = create_stripe_product();
    type ProductCreateRequest =
        <PostgresDataProvider<Product> as DataProvider<Product>>::CreateRequest;
    products
        .create(ProductCreateRequest {
            id: Uuid::new_v4(),
            name: req.name,
            description: req.description.unwrap_or_default(),
            stripe_price_id,
        })
        .await
        .unwrap()
}
fn create_stripe_product() -> String {
    todo!()
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

#[derive(Serialize, Deserialize, Debug)]
pub struct ProcessStripeEvent {
    event: Event,
}

pub type StripeSecret = String;
pub async fn stripe_event(
    request: Request,
    data_providers: tailwag_orm::data_definition::exp_data_system::DataSystem,
    // stripe_signature: Header<StripeSignature>,
    // order_processing_queue: Data<UnboundedSender<ThreadCommand<stripe::WebhookEvent>>>,
    webhook_secret: ServerData<StripeSecret>,
    mut task_queuer: TaskScheduler,
) -> impl IntoResponse {
    /// Verify / Decode the stripe event
    let Some(stripe_signature) = request.headers.get("stripe-signature").cloned() else {
        return Response::not_found();
    };
    // let body: stripe::Event = <stripe::Event as FromRequest>::from(request); // TODO: INterface improvments. Use actual "From<Request>"
    let tailwag_web_service::application::http::route::HttpBody::Json(body) = request.body else {
        return Response::bad_request();
    };
    let event = match stripe::Webhook::construct_event(&body, &stripe_signature, &webhook_secret) {
        Ok(event) => event,
        Err(err) => {
            log::error!("[STRIPE] Failed to unpack stripe event: {}", err.to_string());
            return Response::bad_request();
        },
    };
    let event_id = event.id.clone();
    log::debug!("[STRIPE] Received event id: {event_id}");

    /// Send the event to our event processor.
    /// TODO: This can be one line if I just add '?' support to Response / IntoResponse
    let Ok(ticket) = task_queuer.enqueue(ProcessStripeEvent {
        event,
    }) else {
        log::error!("[TICKET CREATE FAILED] Failed to send task to handler.");
        return Response::internal_server_error();
    };
    log::info!("Created ticket: {}", &ticket.id());

    Response::ok()
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct CartItem {
    id: Uuid,
    quantity: usize,
    // TODO:
    // customizations: Vec<Customization>,
}

pub mod checkout {
    use std::collections::HashMap;

    use crate::{CartItem, Product, ShopOrder};
    use tailwag_orm::{
        data_definition::exp_data_system::DataSystem,
        data_manager::{traits::DataProvider, PostgresDataProvider},
        queries::filterable_types::FilterEq,
    };
    use tailwag_web_service::application::http::route::{IntoResponse, Response, ServerData};

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
                        stripe::CreateCheckoutSessionShippingAddressCollectionAllowedCountries::Us,
                    ],
                },
            ),
            automatic_tax: Some(stripe::CreateCheckoutSessionAutomaticTax {
                enabled: true,
                liability: None,
            }),
            payment_intent_data: Some(stripe::CreateCheckoutSessionPaymentIntentData {
                receipt_email: None,
                ..Default::default()
            }),
            client_reference_id: Some(order_id),
            mode: Some(stripe::CheckoutSessionMode::Payment),
            line_items: Some(
                products
                    .iter()
                    .map(|li| stripe::CreateCheckoutSessionLineItems {
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

        stripe::CheckoutSession::create(stripe_client, params).await.unwrap()
    }
}

pub fn handle_stripe_event(
    event: ProcessStripeEvent,
    // orders: PostgresDataProvider<ShopOrder>,
) -> String {
    log::info!("Recevied event: {:?}", event);
    format!("FOUND: {:?}", event)
}

#[tokio::main]
async fn main() {
    // Example: I want to set up a worker process for the completed stripe events:
    let mut task_executor = TaskExecutor::default();
    task_executor.add_handler(handle_stripe_event);
    let scheduler = task_executor.scheduler();
    // let join_handle = task_executor.run_in_new_thread();

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
        .with_server_data::<StripeSecret>(
            // Hardcoded for test -
            // URGENT TODO: Extract this into a ocnfig
            "whsec_c30a4b7e8df448adfc8009ac03c967a8c0ce6d64b2fd855e61d7f24b37509afd".to_string(),
        )
        // .with_queued_task(handle_stripe_event)
        .with_server_data(scheduler)
        .build_service()
        .run()
        .await
        .unwrap();

    // join_handle.join();
}
