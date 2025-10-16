# gcplog-rs

![Crates.io](https://img.shields.io/crates/v/testscript-rs)

A Rust tracing subscriber that emits logs to stderr as newline-delimited JSON in the format expected by Google Cloud Logging.

## Usage

```rust
use tracing::info;

fn main() {
    // Auto-detect project ID from metadata service
    gcplog_rs::init(gcplog_rs::Config::new());

    info!("Application started");
}
```

With explicit project ID:

```rust
gcplog_rs::init(gcplog_rs::Config::with_project_id("my-project-123"));
```

With custom log level:

```rust
use tracing_subscriber::filter::LevelFilter;

gcplog_rs::init(
    gcplog_rs::Config::with_project_id("my-project-123")
        .with_level(LevelFilter::DEBUG)
);
```

## Trace Correlation

To correlate logs with traces, create spans with a `trace_id` field:

```rust
use tracing::info_span;

let span = info_span!("trace_id", trace_id = %"abc123");
let _guard = span.enter();

info!("This log will include the trace ID");
```

## Output Format

Logs are emitted as JSON:

```json
{
  "severity": "INFO",
  "message": "Application started",
  "time": "2025-10-15T21:05:13.661Z",
  "logging.googleapis.com/sourceLocation": {
    "file": "src/main.rs",
    "line": "10",
    "function": "my_app"
  },
  "logging.googleapis.com/trace": "projects/my-project/traces/abc123"
}
```
