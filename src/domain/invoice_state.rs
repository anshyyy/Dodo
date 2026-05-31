use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "invoice_state", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum InvoiceState {
    Draft,
    Open,
    Paid,
    Void,
    Uncollectible,
}

impl InvoiceState {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Paid | Self::Void | Self::Uncollectible)
    }

    /// Initial state when creating an invoice (`None` means open).
    pub fn from_create_option(raw: Option<&str>) -> Result<Self, InvalidCreateInvoiceState> {
        match raw {
            None | Some("open") => Ok(Self::Open),
            Some("draft") => Ok(Self::Draft),
            Some(_) => Err(InvalidCreateInvoiceState),
        }
    }

    pub fn from_filter_str(raw: &str) -> Result<Self, InvalidInvoiceStateFilter> {
        match raw {
            "draft" => Ok(Self::Draft),
            "open" => Ok(Self::Open),
            "paid" => Ok(Self::Paid),
            "void" => Ok(Self::Void),
            "uncollectible" => Ok(Self::Uncollectible),
            _ => Err(InvalidInvoiceStateFilter),
        }
    }
}

#[derive(Debug, Clone, Copy, thiserror::Error)]
#[error("state must be open or draft")]
pub struct InvalidCreateInvoiceState;

#[derive(Debug, Clone, Copy, thiserror::Error)]
#[error("unknown state filter")]
pub struct InvalidInvoiceStateFilter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvoiceEvent {
    Finalize,
    Void,
    MarkUncollectible,
    PaymentSucceeded,
    PaymentFailed,
}

#[derive(Debug, thiserror::Error)]
#[error("invalid transition from {from:?} on event {event:?}")]
pub struct InvalidTransition {
    pub from: InvoiceState,
    pub event: InvoiceEvent,
}

impl InvoiceState {
    pub fn apply(self, event: InvoiceEvent) -> Result<Self, InvalidTransition> {
        use InvoiceEvent::{Finalize, MarkUncollectible, PaymentFailed, PaymentSucceeded, Void as VoidEvent};
        use InvoiceState::{Draft, Open, Paid, Uncollectible, Void};
        let next = match (self, event) {
            (Draft, Finalize) => Open,
            (Draft, VoidEvent) => Void,
            (Open, PaymentSucceeded) => Paid,
            (Open, VoidEvent) => Void,
            (Open, MarkUncollectible) => Uncollectible,
            (Open, PaymentFailed) => Open,
            (from, ev) => return Err(InvalidTransition { from, event: ev }),
        };
        Ok(next)
    }
}
