pub struct HttpResponse {
    pub(crate) body: HttpResponseBody,
}

type HttpResponseBody = String;
