use tiny_http::{Response, Server};
use tracing::{info, info_span};

fn main() {
    // Initialize GCP logging - will auto-detect project ID
    gcplog_rs::init(gcplog_rs::Config::new());

    info!("Starting HTTP server on port 8080");

    let server = Server::http("0.0.0.0:8080").unwrap();
    info!("Server listening on 0.0.0.0:8080");

    for request in server.incoming_requests() {
        let trace_id = request
            .headers()
            .iter()
            .find(|h| h.field.equiv("traceparent"))
            .map(|h| h.value.to_string());

        let span = if let Some(tid) = trace_id {
            info_span!("trace_id", trace_id = %tid)
        } else {
            info_span!("no trace_id")
        };
        let _guard = span.enter();

        info!("received request: {} {}", request.method(), request.url());

        let response = Response::from_string("Hello, World!");
        if let Err(e) = request.respond(response) {
            tracing::error!(error = ?e, "failed to send response");
        } else {
            info!("response sent");
        }
    }
}
