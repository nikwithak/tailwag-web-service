use serde::{Deserialize, Serialize};
use stripe::{CheckoutSessionPaymentStatus, Event, EventObject, StripeError};
use tailwag_macros::Display;
use tailwag_orm::{
    data_manager::{traits::DataProvider, PostgresDataProvider},
    queries::filterable_types::FilterEq,
};
use tailwag_web_service::{
    application::http::route::{IntoResponse, Request, Response, ServerData},
    auth::gateway::{self, authorize_request},
    tasks::TaskScheduler,
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

#[derive(
    Clone,
    Debug,
    Default,
    serde::Deserialize,
    serde::Serialize,
    sqlx::FromRow,
    tailwag::macros::GetTableDefinition,
    tailwag::macros::Insertable,
    tailwag::macros::Updateable,
    tailwag::macros::Deleteable,
    tailwag::macros::Filterable,
    tailwag::macros::BuildRoutes,
    tailwag::macros::Id,
    tailwag::macros::Display,
    tailwag::forms::macros::GetForm,
)]
#[create_type(CreateProductRequest)]
#[post(create_product)]
pub struct Product {
    #[no_form]
    id: Uuid,
    name: String,
    description: String,
    #[no_form]
    stripe_price_id: String,
    price_usd_cents: i64,
}

/// Here is an example of overriding the CreateRequest type.
/// In this example, we want to create the product with the Stripe API
/// as a part of the create process, which we can't do without a custom type.
///
/// To accomplish this custom create implementation (Affecting the `POST` operation),
/// we do three things:
///
/// 1. Define the type, making sure it is Serializable, Deserializeable, and Cloneable.
/// 2. Implement Into<Product> (via a From<> impl)
/// 3. Assign it as the create_request type with the #[create_type(CreateProductRequest)]
///    attribute, in the #[derive(Insertable)] implementation.
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct CreateProductRequest {
    name: String,
    description: String,
    price_usd_cents: u64,
}

impl From<CreateProductRequest> for Product {
    fn from(val: CreateProductRequest) -> Self {
        Product {
            id: uuid::Uuid::new_v4(),
            name: val.name,
            description: val.description,
            stripe_price_id: "".to_owned(),
            price_usd_cents: val.price_usd_cents as i64,
        }
    }
}

// TODO: Find a way to move this to the DataProvider. Using the standard From<> trait makes this not really work.
pub async fn create_product(
    req: CreateProductRequest,
    products: PostgresDataProvider<Product>,
) -> Response {
    let mut product = products.create(req).await.unwrap();
    if create_stripe_product(&mut product).await.is_err() {
        return Response::internal_server_error();
    }
    if products.update(&product).await.is_err() {
        return Response::internal_server_error();
    };
    product.into_response()
}

///  Creates a new product on Stripe. Requires the secret be configured.
async fn create_stripe_product(product: &mut Product) -> Result<(), StripeError> {
    // let stripe_client = stripe_client.lock().await;
    // TODO: Should I store stripe_client as a Data obj?? Probably!!
    let stripe_secret_key = std::env::var("STRIPE_API_KEY").expect("No secret key configured");
    let stripe_client = stripe::Client::new(stripe_secret_key);
    let id = product.id.to_string();

    let stripe_product = stripe::CreateProduct {
        active: Some(true),
        description: None,
        expand: &[],
        id: Some(&id),
        images: None,
        // images: Some(Box::new(self.image_urls.clone())),
        metadata: None,
        name: product.name.as_str(),
        package_dimensions: None,
        shippable: Some(true),
        statement_descriptor: None,
        tax_code: None,
        unit_label: None,
        url: None,
        default_price_data: Some(stripe::CreateProductDefaultPriceData {
            currency: stripe::Currency::USD,
            currency_options: None,
            recurring: None,
            tax_behavior: Some(stripe::CreateProductDefaultPriceDataTaxBehavior::Exclusive),
            unit_amount: Some(product.price_usd_cents),
            unit_amount_decimal: None,
        }),
        features: None,
        type_: None,
    };

    let stripe_product = stripe::Product::create(&stripe_client, stripe_product).await?;

    match &stripe_product.default_price {
        Some(price) => {
            product.stripe_price_id = price.id().to_string();
            Ok(())
        },
        None => {
            Err(stripe::StripeError::ClientError("Failed to create price at Stripe.".to_string()))
        },
    }
}

#[derive(
    Clone,
    Debug,
    Default,
    serde::Deserialize,
    serde::Serialize,
    sqlx::FromRow,
    tailwag::macros::GetTableDefinition,
    tailwag::macros::Insertable,
    tailwag::macros::Updateable,
    tailwag::macros::Deleteable,
    tailwag::macros::Filterable,
    tailwag::macros::BuildRoutes,
    tailwag::macros::Id,
    tailwag::macros::Display,
    tailwag::forms::macros::GetForm,
)]
#[actions(stripe_event)]
pub struct ShopOrder {
    id: Uuid,
    customer_name: String,
    customer_email: String,
    status: String,
    stripe_session_id: String,
    // #[no_filter]
    // #[sqlx(skip)]
    // product: Product,
}
#[derive(Display)]
#[allow(unused)]
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
    _data_providers: tailwag_orm::data_definition::exp_data_system::DataSystem,
    webhook_secret: ServerData<StripeSecret>,
    mut task_queuer: TaskScheduler,
) -> impl IntoResponse {
    /// Verify / Decode the stripe event
    let Some(stripe_signature) = request.headers.get("stripe-signature").cloned() else {
        return Response::not_found();
    };
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
    /// TODO [TECH DEBT]: This can be one line if I just figure out how to add '?' support to Response / IntoResponse
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
        // .with_middleware(authorize_request)
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
}
