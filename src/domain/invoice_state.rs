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
}

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
