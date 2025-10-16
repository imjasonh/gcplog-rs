use chrono::{SecondsFormat, Utc};
use serde::Serialize;
use serde_json::to_string;
use std::env;
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id};
use tracing::{Event, Subscriber};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::Context;
use tracing_subscriber::prelude::*;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::{registry, Layer};

struct TraceId(String);

#[derive(Default)]
struct TraceIdVisitor {
    trace_id: Option<String>,
}

impl Visit for TraceIdVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "trace_id" {
            self.trace_id = Some(format!("{value:?}"))
        }
    }
}

#[derive(Default)]
struct EventVisitor {
    message: Option<String>,
}

impl Visit for EventVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = Some(format!("{value:?}"))
        }
    }
}

struct GcpLayer {
    gcp_project_id: String,
}

#[derive(Serialize)]
struct SourceLocation {
    file: String,
    line: String,
    function: String,
}

#[derive(Serialize)]
struct LogEntry<'a> {
    severity: &'a str,
    message: String,
    time: String,
    #[serde(rename = "logging.googleapis.com/trace")]
    #[serde(skip_serializing_if = "Option::is_none")]
    trace: Option<String>,
    #[serde(rename = "logging.googleapis.com/sourceLocation")]
    #[serde(skip_serializing_if = "Option::is_none")]
    source_location: Option<SourceLocation>,
}

impl<S> Layer<S> for GcpLayer
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(id) {
            let mut visitor = TraceIdVisitor::default();
            attrs.record(&mut visitor);
            if let Some(trace_id) = visitor.trace_id {
                span.extensions_mut().insert(TraceId(trace_id));
            }
        };
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let mut trace = None;
        if let Some(scope) = ctx.event_scope(event) {
            for span in scope.from_root() {
                let extensions = span.extensions();
                let Some(trace_id) = extensions.get::<TraceId>() else {
                    continue;
                };
                let t = &trace_id.0;
                trace = Some(format!("projects/{}/traces/{t}", self.gcp_project_id));
            }
        }
        let mut visitor = EventVisitor::default();
        event.record(&mut visitor);

        let metadata = event.metadata();
        let source_location = metadata.file().map(|file| SourceLocation {
            file: file.to_string(),
            line: metadata.line().unwrap_or(0).to_string(),
            function: metadata.target().to_string(),
        });

        let entry = LogEntry {
            severity: event.metadata().level().as_str(),
            message: visitor.message.unwrap_or_default(),
            time: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            trace,
            source_location,
        };
        eprintln!("{}", to_string(&entry).unwrap());
    }
}

/// Fetch the GCP project ID from the metadata service.
///
/// This queries the GCP metadata service at http://169.254.169.254/computeMetadata/v1/project/project-id
/// The metadata host can be overridden via the GCE_METADATA_HOST environment variable.
fn fetch_project_id() -> Result<String, Box<dyn std::error::Error>> {
    let host = env::var("GCE_METADATA_HOST").unwrap_or_else(|_| "169.254.169.254".to_string());
    let url = format!("http://{}/computeMetadata/v1/project/project-id", host);

    let response = ureq::get(&url)
        .set("Metadata-Flavor", "Google")
        .timeout(std::time::Duration::from_secs(2))
        .call()?;

    let project_id = response.into_string()?.trim().to_string();
    Ok(project_id)
}

/// Configuration for the GCP structured logging subscriber.
pub struct Config {
    /// Your GCP project ID used to construct the full trace path.
    /// If None, will attempt to fetch from the GCP metadata service.
    pub gcp_project_id: Option<String>,
    /// The minimum log level to emit (defaults to INFO if not specified)
    pub level_filter: Option<LevelFilter>,
}

impl Config {
    /// Create a new config that will auto-detect the GCP project ID from the metadata service.
    /// The log level will default to INFO.
    pub fn new() -> Self {
        Self {
            gcp_project_id: None,
            level_filter: None,
        }
    }

    /// Create a new config with the specified GCP project ID.
    /// The log level will default to INFO.
    pub fn with_project_id(gcp_project_id: impl Into<String>) -> Self {
        Self {
            gcp_project_id: Some(gcp_project_id.into()),
            level_filter: None,
        }
    }

    /// Set the log level filter.
    pub fn with_level(mut self, level: LevelFilter) -> Self {
        self.level_filter = Some(level);
        self
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::new()
    }
}

/// Initialize the GCP structured logging subscriber.
///
/// This sets up a tracing subscriber that outputs logs to stderr in the JSON format
/// expected by Google Cloud Run and Cloud Logging. To associate logs with traces,
/// create spans with a `trace_id` field using `info_span!("trace_id", trace_id = %"your-trace-id")`.
///
/// If the config does not specify a project ID, this will attempt to fetch it from the
/// GCP metadata service. If that fails, it will use "unknown" as the project ID.
///
/// # Arguments
///
/// * `config` - Configuration for the subscriber
///
/// # Examples
///
/// ```no_run
/// use tracing::info;
///
/// // Auto-detect project ID from metadata service
/// gcplog_rs::init(gcplog_rs::Config::new());
/// info!("Application started");
/// ```
///
/// ```no_run
/// use tracing::info;
///
/// // Use explicit project ID
/// gcplog_rs::init(gcplog_rs::Config::with_project_id("my-project-123"));
/// info!("Application started");
/// ```
///
/// ```no_run
/// use tracing::info;
/// use tracing_subscriber::filter::LevelFilter;
///
/// // Use custom level
/// let config = gcplog_rs::Config::with_project_id("my-project-123")
///     .with_level(LevelFilter::DEBUG);
/// gcplog_rs::init(config);
/// info!("Application started");
/// ```
pub fn init(config: Config) {
    let gcp_project_id = config
        .gcp_project_id
        .or_else(|| fetch_project_id().ok())
        .unwrap_or_else(|| "unknown".to_string());

    let layer = GcpLayer { gcp_project_id };
    let level_filter = config.level_filter.unwrap_or(LevelFilter::INFO);
    registry().with(layer.with_filter(level_filter)).init();
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing::{info, info_span, warn};

    #[test]
    fn test_gcp_log_format() {
        init(Config::with_project_id("test-project-123"));

        // Test basic log without trace
        info!("Application started");

        // Test log with trace_id
        let span = info_span!("trace_id", trace_id = %"abc123");
        let _guard = span.enter();
        info!("Processing request");
        warn!("Potential issue detected");
    }
}
