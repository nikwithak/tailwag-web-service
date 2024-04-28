use std::sync::Arc;

use serde::{Deserialize, Serialize};
use stripe::{CheckoutSessionPaymentStatus, Event, EventObject};
use tailwag_macros::{derive_magic, Display};
use tailwag_orm::{
    data_manager::{traits::DataProvider, PostgresDataProvider},
    queries::filterable_types::FilterEq,
};
use tailwag_web_service::{
    application::http::route::{FromRequest, IntoResponse, Request, Response, ServerData},
    auth::gateway,
    tasks::{
        runner::{TaskExecutor, TaskResult},
        TaskScheduler,
    },
};
use tokio::sync::watch::error;
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
        // TODO: I need to fix enums in the ORM first. ORM tightening is going to be next major feature, it's really holidng me back.
        // status: ShopOrderStatus,
        status: String,
        stripe_session_id: String,
        // line_items: Vec<String>, // TODO: Fix thiiiis
        // name: String,
        // description: String,
    }
}
#[derive(Display)]
enum ShopOrderStatus {
    Created,
    Canceled,
    Paid,
    Shipped,
    Delivered,
    Completed,
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

pub async fn handle_stripe_event(
    event: ProcessStripeEvent,
    orders: PostgresDataProvider<ShopOrder>,
) -> String {
    log::info!("[STRIPE] Received event in task queue: {:?}", event);
    process_event(&event.event, orders).await;
    "Finished.".to_string()
}

async fn process_checkout_session_completed_event(
    event: &stripe::Event,
    orders: impl DataProvider<ShopOrder>,
) -> Result<(), tailwag_web_service::Error> {
    let EventObject::CheckoutSession(session) = &event.data.object else {
        Err("Invalid checkout session received.")?
    };
    let Some(order_id) = session
        .client_reference_id
        .as_ref()
        .and_then(|oid| uuid::Uuid::parse_str(oid).ok())
    else {
        Err("Invalid order ID received")?
    };
    let Ok(Some(mut order)) = orders.get(|o| o.id.eq(order_id)).await else {
        Err("Could not find order in DB")?
    };

    order.stripe_session_id = session.id.to_string();
    match session.payment_status {
        CheckoutSessionPaymentStatus::Paid => order.status = ShopOrderStatus::Paid.to_string(),
        CheckoutSessionPaymentStatus::Unpaid => {
            order.status = ShopOrderStatus::Canceled.to_string()
        },
        CheckoutSessionPaymentStatus::NoPaymentRequired => {
            order.status = ShopOrderStatus::Paid.to_string()
        },
    }

    if orders.update(&order).await.is_err() {
        log::error!("Failed to update order {}", &order_id);
    };
    log::info!("Order {order_id} status updated to {}", order_id);

    // if let Some(customer) = &session.customer_details {
    //     let customer = &**customer;
    //     order.customer_detail = Some(CustomerDetail {
    //         // customer_name: customer.name,
    //         customer_name: order.customer_detail.map_or(None, |c| c.customer_name),
    //         customer_phone: customer.phone.as_ref().map(|p| p.to_string()),
    //         customer_email: customer.email.as_ref().map(|e| e.to_string()),
    //     });
    // }

    // if let Some(stripe_shipping) = &session.shipping {
    //     order.shipping_detail = Some(ShippingInfo::from(&**stripe_shipping));
    // }
    // order.customer_phone = session.customer_details.as_ref().map_or(None,
    // |detail| { detail.phone.map_or(None, |p|
    // Some(*p.clone()) });
    // if let Some(customer) = session.customer {
    //     match *customer {
    //         stripe::Expandable::Object(customer) => {
    //             order.customer_name = customer.name.map(|n| *n.clone());
    //         },
    //         stripe::Expandable::Id(id) => {
    //             order.customer_name =  Some(id.to_string());
    //             log::info!("Order processed for customer_id {}", id);
    //         }
    //     }
    // } else {
    //     log::info!("No customer found");
    // }

    // order.amount = Some(OrderAmount::from(&session));

    // Send emails if it hasn't already been sent
    // if !order.confirmation_email_sent {
    //     if let Some(email) =
    //         order.customer_detail.as_ref().map(|c| c.customer_email.as_ref()).flatten()
    //     {
    //         // TODO: Externalize content
    //         match send_email(
    //             &email,
    //             "Your Scrapplique Order",
    //             &get_order_confirmation_email_content(&order),
    //         )
    //         .await
    //         {
    //             Err(e) => {
    //                 log::error!("Failed to send order confirmation email {}", e);
    //             },
    //             _ => log::info!("Send e mail for order id {}", &order.id),
    //         };
    //     } else {
    //         log::error!("Processed order without customer email: {}", &order.id);
    //     }

    //     // Send email to Business
    //     // TODO: Externalize this
    //     match send_email(
    //         "nik@tonguetail.com",
    //         "You have a new order!!",
    //         "Visit https://beta.scrapplique.com/manage/orders to view it",
    //     )
    //     .await
    //     {
    //         Err(e) => {
    //             log::error!("Failed to send order confirmation email {}", e);
    //         },
    //         _ => log::info!("Send e mail for order id {}", &order.id),
    //     };
    //     order.confirmation_email_sent = true;
    // }

    // dbg!(&order).save(&db.pool).await?;
    // Ok(())
    Ok(())
}
pub async fn process_event(
    event: &stripe::Event,
    orders: impl DataProvider<ShopOrder>,
) -> Result<(), tailwag_web_service::Error> {
    let stripe::Event {
        id,
        type_,
        ..
    } = &event;

    match type_ {
        stripe::EventType::CheckoutSessionCompleted => {
            process_checkout_session_completed_event(event, orders).await
        },
        _ => {
            log::info!("Ignoring webhook event {}", serde_json::to_string(type_)?);
            Ok(())
        },
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
        .with_server_data::<StripeSecret>(
            std::env::var("STRIPE_WEBHOOK_SECRET").expect("STRIPE_WEBHOOK_SECRET missing"),
        )
        .with_task(handle_stripe_event)
        .build_service()
        .run()
        .await
        .unwrap();

    // join_handle.join();
}
