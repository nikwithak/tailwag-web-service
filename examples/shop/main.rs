use serde::{Deserialize, Serialize};
use stripe::StripeError;
use stripe_integration::stripe_event;
use tailwag_macros::Display;
use tailwag_orm::data_manager::{traits::DataProvider, PostgresDataProvider};
use tailwag_web_service::{
    application::http::route::{IntoResponse, RequestContext, Response},
    auth::gateway::{self, authorize_request, Session},
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
        id: Some(&id),
        name: product.name.as_str(),
        active: Some(true),
        default_price_data: Some(stripe::CreateProductDefaultPriceData {
            currency: stripe::Currency::USD,
            currency_options: None,
            recurring: None,
            tax_behavior: Some(stripe::CreateProductDefaultPriceDataTaxBehavior::Exclusive),
            unit_amount: Some(product.price_usd_cents),
            unit_amount_decimal: None,
        }),
        images: None,
        description: None,
        expand: &[],
        // images: Some(Box::new(self.image_urls.clone())),
        metadata: None,
        package_dimensions: None,
        shippable: Some(true),
        statement_descriptor: None,
        tax_code: None,
        unit_label: None,
        url: None,
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
    confirmation_email_sent: bool,
    // #[no_filter]
    #[create_ignore]
    #[no_filter]
    #[no_form]
    order_amount: OrderAmount,
    // TODO: once I implement flatten / other types, this will be easier.
    // amount_subtotal: i64,
    // amount_tax: i64,
    // amount_shipping: i64,
    // amount_discount: i64,
    // amount_total: i64,
}

#[derive(
    Clone,
    Debug,
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
pub struct OrderAmount {
    id: Uuid,
    subtotal_amount: i64,
    tax_amount: i64,
    shipping_amount: i64,
    discount_amount: i64,
    total_amount: i64,
}
impl Default for OrderAmount {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            subtotal_amount: Default::default(),
            tax_amount: Default::default(),
            shipping_amount: Default::default(),
            discount_amount: Default::default(),
            total_amount: Default::default(),
        }
    }
}

impl From<&stripe::CheckoutSession> for OrderAmount {
    fn from(stripe_session: &stripe::CheckoutSession) -> Self {
        let subtotal_amount = stripe_session.amount_subtotal.as_ref().map_or(0, |b| *b);
        let total_amount = stripe_session.amount_total.as_ref().map_or(0, |b| *b);
        let (tax_amount, shipping_amount, discount_amount) =
            stripe_session.total_details.as_ref().map_or((0, 0, 0), |amounts| {
                (
                    amounts.amount_tax,
                    amounts.amount_shipping.as_ref().map_or(0, |b| *b),
                    amounts.amount_discount,
                )
            });

        Self {
            id: Uuid::new_v4(),
            subtotal_amount,
            tax_amount,
            shipping_amount,
            discount_amount,
            total_amount,
        }
    }
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

#[derive(Serialize, Deserialize, Debug)]
pub struct SendEmailEvent {
    pub subject: String,
    pub body: String,
    pub recipient: String,
}

pub async fn email_webhook(
    request: SendEmailEvent,
    _data_providers: tailwag_orm::data_definition::exp_data_system::DataSystem,
    mut task_queuer: TaskScheduler,
    ctx: RequestContext,
) -> impl IntoResponse {
    if ctx.get_request_data::<Session>().is_none() {
        return Response::unauthorized();
    }
    if task_queuer.enqueue(request).is_ok() {
        Response::ok()
    } else {
        Response::internal_server_error()
    }
}

pub async fn send_email(event: SendEmailEvent) {
    let SendEmailEvent {
        subject,
        body,
        recipient,
    } = event;
    let client = tailwag_utils::email::sendgrid::SendGridEmailClient::from_env().unwrap();
    client.send_email(&recipient, &subject, &body).await.unwrap();
}

pub mod stripe_integration;

#[tokio::main]
async fn main() {
    tailwag_web_service::application::WebService::builder("My Events Service")
        .with_middleware(authorize_request)
        .post_public("/login", gateway::login)
        .post_public("/register", gateway::register)
        .with_resource::<Product>() // TODO- public GET with filtering)
        .with_resource::<ShopOrder>() // TODO - public POST, restricted GET (to specific customer, via email)
        .with_resource::<OrderAmount>() // TODO - Needed to make sure the tables get created. TODO: Auto-create all direct dependent tables automatically in the ORM
        // .with_resource::<Customer>() // TODO - No public - (nly created from stripe stuff, for now)
        .post_public("/checkout", checkout::checkout) // TODO
        .post_public("/email", email_webhook)
        .with_server_data(stripe::Client::new(
            std::env::var("STRIPE_API_KEY").expect("STRIPE_API_KEY is missing from env."),
        ))
        .with_task(stripe_integration::handle_stripe_event)
        .with_task(send_email)
        .with_static_files()
        .build_service()
        .run()
        .await
        .unwrap();
}
