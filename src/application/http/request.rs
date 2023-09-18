use super::{response::HttpResponse, routes::RoutePath, HttpHeader, ToJsonString};

pub struct TailwagApplicationRequest {
    request: HttpRequest,
    state: String, // TODO
    context: RequestContext,
}

pub struct RequestContext;

pub struct HttpRequest {
    body: HttpRequestBody,
    method: HttpMethod,
    headers: HttpHeader,
    path: RoutePath,
    // .. add here as needed
}

fn handle_http_request(req: HttpRequest) {}

pub trait HttpRequestHandler<Req> {
    fn handle_request(
        &self,
        request: HttpRequest,
    ) -> HttpResponse;
}

// TODO: Finish this - the whole mod is kinda WIP right now.
impl<Function, Req, Res> HttpRequestHandler<Req> for Function
where
    Function: Fn(Req) -> Res,
    Req: Send + From<String>,
    Res: ToJsonString, // Really just make
{
    fn handle_request(
        &self,
        request: HttpRequest,
    ) -> HttpResponse {
        let request_body: Req = request.body.into();
        let response = self(request_body);
        HttpResponse {
            body: response.to_json_string(),
        }
    }
}

type HttpRequestBody = String;
pub enum HttpMethod {
    Get,
    Post,
    Patch,
    Put,
    Delete,
    // Options,
    // ..
    // TODO: Add the rest (no pun intended)
}
