//! Production-style logging via [`tracing`] + [`tracing-subscriber`].
//!
//! - Filter: `RUST_LOG` (e.g. `dodo_invoice_service=info,tower_http=info`).
//! - Format: `LOG_FORMAT=json` for one-line JSON (Datadog, CloudWatch, etc.); default is human-readable.

use std::env;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

pub fn init() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "dodo_invoice_service=info,tower_http=info".into());

    let json_logs = env::var("LOG_FORMAT")
        .map(|v| v.eq_ignore_ascii_case("json"))
        .unwrap_or(false);

    if json_logs {
        tracing_subscriber::registry()
            .with(filter)
            .with(
                fmt::layer()
                    .json()
                    .flatten_event(true)
                    .with_current_span(false)
                    .with_span_list(false),
            )
            .init();
    } else {
        tracing_subscriber::registry()
            .with(filter)
            .with(fmt::layer().with_target(true))
            .init();
    }
}
