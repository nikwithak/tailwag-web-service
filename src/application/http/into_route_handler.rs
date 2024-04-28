/// This mod contains all the logic / trait impls for automatically converting functions into a RouteHandler.
/// The goal is to enable ergonomic and intuitive route handling.
/// At the moment, it supports exactly one Request input type, and one that reads from the Context (which currently only contains data providers).
use std::pin::Pin;

use std::future::Future;

use crate::application::http::route::{FromRequest, IntoResponse, RouteHandler, ServerContext};

impl IntoRouteHandler<(), (), ()> for RouteHandler {
    fn into(self) -> RouteHandler {
        self
    }
}

pub struct RouteArgsStaticRequest;

/// The generics are merely here for tagging / distinguishing implementations.
/// F: represents the function signature for the different implementations. This is the one that really matters.
/// Tag: Merely tag structs, to disambiguate implementations when there is trait overlap.
/// IO: The function input / output types. They must be a part of the trait declaration in order to be used in the impl,
///     i.e. these exist only so that we can use them to define `F`
pub trait IntoRouteHandler<F, Tag, IO> {
    fn into(self) -> RouteHandler;
}
impl<F, I, O> IntoRouteHandler<F, RouteArgsStaticRequest, (I, O)> for F
where
    F: Fn(I) -> Pin<Box<dyn Send + 'static + std::future::Future<Output = O>>>
        + Send
        + Sync
        + Copy
        + 'static,
    I: FromRequest + Sized + Send,
    O: IntoResponse + Sized + Send,
{
    fn into(self) -> RouteHandler {
        RouteHandler {
            handler: Box::new(move |req, _ctx| {
                Box::pin(async move { self(I::from(req)).await.into_response() })
            }),
        }
    }
}
pub struct RouteArgsNone;
impl<F, O> IntoRouteHandler<F, RouteArgsNone, O> for F
where
    F: Fn() -> O + Send + Copy + 'static + Sync,
    O: IntoResponse + Sized + Send + Sync,
{
    fn into(self) -> RouteHandler {
        RouteHandler {
            handler: Box::new(move |_, _| Box::pin(async move { self().into_response() })),
        }
    }
}

// Let's break that down.

// We define the following Generics:
//     GENERICS: F, I, an O.

// For a breakdown of the

// F is the function type, and the main type that we are implementing the IntoRH for. I is the input type of F, O is the output type of F.

// We define IntoRouteHanlder in terms of F (The function type we want to use as a handler),
// Tag (RouteArgsStaticRequest, in this case), and the input/output types.

// So... why do we need so many generics, to all do the same thing? We need I/O to be generic in order to define them
// in terms of the FromRequest and IntoResponse trait.
// Because of a restriction imposed by the compiler, we can't use a generic in the implemnetation unles sit's also a generic in
// either the trait, or the struct implementing the trait.

// Unfortunately... this doesn't flow into the `where` clauses - which is to say, we can't do a generic implemnetation *over* a generic struct. That's why we have
// to define IntoRouteHandler (and not getting the benefits of Into<RouteHandler>)

// So where does RouteArgsStaticRequest (the Tag) come in? The tag was to get around a restriction of multiple implementations
// using the same or similar generics, which adds ambiguity. As the developer, I can reasonably assume
// that the implementations are unique, at least for my specific use cases, but the compiler doesn't
// know how to cope with the other cases, since it is possible for the generics of `F(I) -> O` to overlap both.

// The Tag ensures that the compiler will magically choose the right implementation, if only one applies.
// In the event that a class overlaps in actual usage, then the user will have to disambiuate using these tags.

// As a user, you shouldn't have to ever worry or care about these weird generics - this
// abstraction is intended ot make coding with this library more ergonomic over closures
// and simple function types. This explanation is only here for those curious enough to look under the hood.

pub struct RouteArgsNoContextAsync;
impl<F, I, O, Fut> IntoRouteHandler<F, RouteArgsNoContextAsync, (F, I, (O, Fut))> for F
where
    F: Fn(I) -> Fut + Send + Copy + 'static + Sync,
    I: FromRequest + Sized + 'static,
    O: IntoResponse + Sized + Send + 'static,
    Fut: Future<Output = O> + 'static + Send,
{
    fn into(self) -> RouteHandler {
        RouteHandler {
            handler: Box::new(move |req, _ctx| {
                Box::pin(async move { self(I::from(req)).await.into_response() })
            }),
        }
    }
}

pub struct RouteArgsNoContextSync;
impl<F, I, O> IntoRouteHandler<F, RouteArgsNoContextSync, (F, I, O)> for F
where
    F: Send + Sync + Copy + 'static + Fn(I) -> O,
    I: FromRequest + Sized + 'static,
    O: IntoResponse + Sized + Send + 'static,
{
    fn into(self) -> RouteHandler {
        RouteHandler {
            handler: Box::new(move |req, _ctx| {
                Box::pin(async move { self(I::from(req)).into_response() })
            }),
        }
    }
}

macro_rules! generate_trait_impl {
    (R1, $($context_id:ident),*) => {
        impl<F, I, $($context_id,)* O, Fut>
            IntoRouteHandler<F, (Fut, $($context_id,)*), (($($context_id),*), I, (O, Fut))> for F
        where
            F: Fn(I, $($context_id),*) -> Fut + Send + Copy + 'static + Sync,
            I: FromRequest + Sized + 'static,
            $($context_id: From<ServerContext> + Sized + 'static,)*
            O: IntoResponse + Sized + Send + 'static,
            Fut: Future<Output = O> + 'static + Send,
        {
            fn into(self) -> RouteHandler {
                RouteHandler {
                    handler: Box::new(move |req, ctx| {
                        Box::pin(async move {
                            self(I::from(req), $($context_id::from(ctx.clone())),*)
                                .await
                                .into_response()
                        })
                    }),
                }
            }
        }

        impl<F, I, $($context_id,)* O>
            IntoRouteHandler<F, ($($context_id,)*), (($($context_id),*), I, (O))> for F
        where
            F: Fn(I, $($context_id),*) -> O + Send + Copy + 'static + Sync,
            I: FromRequest + Sized + 'static,
            $($context_id: From<ServerContext> + Sized + 'static,)*
            O: IntoResponse + Sized + Send + 'static,
        {
            fn into(self) -> RouteHandler {
                RouteHandler {
                    handler: Box::new(move |req, ctx| {
                        Box::pin(async move {
                            self(I::from(req), $($context_id::from(ctx.clone())),*)
                                .into_response()
                        })
                    }),
                }
            }
        }
    };
}

// generate_trait_impl!(R1, C1);
generate_trait_impl!(R1, C1, C2);
generate_trait_impl!(R1, C1, C2, C3);
generate_trait_impl!(R1, C1, C2, C3, C4);
generate_trait_impl!(R1, C1, C2, C3, C4, C5);

// pub struct RouteArgsOneRequestTwoContextAsync;
// impl<F, I, C, C2, O, Fut>
//     IntoRouteHandler<F, RouteArgsOneRequestTwoContextAsync, ((C, C2), I, (O, Fut))> for F
// where
//     F: Fn(I, C, C2) -> Fut + Send + Copy + 'static + Sync,
//     I: FromRequest + Sized + 'static,
//     C: From<ServerContext> + Sized + 'static,
//     C2: From<ServerContext> + Sized + 'static,
//     O: IntoResponse + Sized + Send + 'static,
//     Fut: Future<Output = O> + 'static + Send,
// {
//     fn into(self) -> RouteHandler {
//         RouteHandler {
//             handler: Box::new(move |req, ctx| {
//                 Box::pin(async move {
//                     self(I::from(req), C::from(ctx.clone()), C2::from(ctx)).await.into_response()
//                 })
//             }),
//         }
//     }
// }
// pub struct RouteArgsOneRequestTwoContextSync;
// impl<F, I, C, C2, O> IntoRouteHandler<F, RouteArgsOneRequestTwoContextSync, ((C, C2), I, O)> for F
// where
//     F: Send + Copy + 'static + Sync + Fn(I, C, C2) -> O,
//     I: FromRequest + Sized + 'static,
//     C: From<ServerContext> + Sized + 'static,
//     C2: From<ServerContext> + Sized + 'static,
//     O: IntoResponse + Sized + Send + 'static,
// {
//     fn into(self) -> RouteHandler {
//         RouteHandler {
//             handler: Box::new(move |req, ctx| {
//                 Box::pin(async move {
//                     self(I::from(req), C::from(ctx.clone()), C2::from(ctx)).into_response()
//                 })
//             }),
//         }
//     }
// }

pub struct RouteArgsRequestContextAsync;
impl<F, I, C, O, Fut> IntoRouteHandler<F, RouteArgsRequestContextAsync, (C, I, (O, Fut))> for F
where
    F: Fn(I, C) -> Fut + Send + Copy + 'static + Sync,
    I: FromRequest + Sized + 'static,
    C: From<ServerContext> + Sized + 'static,
    O: IntoResponse + Sized + Send + 'static,
    Fut: Future<Output = O> + 'static + Send,
{
    fn into(self) -> RouteHandler {
        RouteHandler {
            handler: Box::new(move |req, ctx| {
                Box::pin(async move { self(I::from(req), C::from(ctx)).await.into_response() })
            }),
        }
    }
}
pub struct RouteArgsRequestContextSync;
impl<F, I, C, O> IntoRouteHandler<F, RouteArgsRequestContextSync, (C, I, O)> for F
where
    F: Send + Copy + 'static + Sync + Fn(I, C) -> O,
    I: FromRequest + Sized + 'static,
    C: From<ServerContext> + Sized + 'static,
    O: IntoResponse + Sized + Send + 'static,
{
    fn into(self) -> RouteHandler {
        RouteHandler {
            handler: Box::new(move |req, ctx| {
                Box::pin(async move { self(I::from(req), C::from(ctx)).into_response() })
            }),
        }
    }
}

pub struct Nothing3Async;
impl<F, C, O, Fut> IntoRouteHandler<F, Nothing3Async, (C, O, Fut)> for F
where
    F: Fn(C) -> Fut + Send + Copy + 'static + Sync,
    C: From<ServerContext> + Sized + 'static,
    O: IntoResponse + Sized + Send + 'static,
    Fut: Future<Output = O> + 'static + Send,
{
    fn into(self) -> RouteHandler {
        RouteHandler {
            handler: Box::new(move |_req, ctx| {
                Box::pin(async move { self(C::from(ctx)).await.into_response() })
            }),
        }
    }
}
pub struct Nothing3Sync;
impl<F, C, O, Fut> IntoRouteHandler<F, Nothing3Sync, (C, O, Fut)> for F
where
    F: Fn(C) -> O + Send + Copy + 'static + Sync,
    C: From<ServerContext> + Sized + 'static,
    O: IntoResponse + Sized + Send + 'static,
{
    fn into(self) -> RouteHandler {
        RouteHandler {
            handler: Box::new(move |_req, ctx| {
                Box::pin(async move { self(C::from(ctx)).into_response() })
            }),
        }
    }
}