-- Businesses and API keys
CREATE TABLE businesses (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE api_keys (
    id UUID PRIMARY KEY,
    business_id UUID NOT NULL REFERENCES businesses(id) ON DELETE CASCADE,
    key_prefix TEXT NOT NULL,
    key_hash TEXT NOT NULL UNIQUE,
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_api_keys_business_id ON api_keys(business_id);

CREATE TABLE customers (
    id UUID PRIMARY KEY,
    business_id UUID NOT NULL REFERENCES businesses(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    email TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (business_id, email)
);

CREATE INDEX idx_customers_business_id ON customers(business_id);

CREATE TYPE invoice_state AS ENUM ('draft', 'open', 'paid', 'void', 'uncollectible');

CREATE TABLE invoices (
    id UUID PRIMARY KEY,
    business_id UUID NOT NULL REFERENCES businesses(id) ON DELETE CASCADE,
    customer_id UUID NOT NULL REFERENCES customers(id) ON DELETE RESTRICT,
    state invoice_state NOT NULL DEFAULT 'open',
    due_date DATE NOT NULL,
    total_cents BIGINT NOT NULL,
    currency TEXT NOT NULL DEFAULT 'USD',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_invoices_business_state ON invoices(business_id, state);

CREATE TABLE invoice_line_items (
    id UUID PRIMARY KEY,
    invoice_id UUID NOT NULL REFERENCES invoices(id) ON DELETE CASCADE,
    description TEXT NOT NULL,
    quantity INTEGER NOT NULL CHECK (quantity > 0),
    unit_amount_cents BIGINT NOT NULL CHECK (unit_amount_cents >= 0),
    line_total_cents BIGINT NOT NULL CHECK (line_total_cents >= 0)
);

CREATE INDEX idx_line_items_invoice_id ON invoice_line_items(invoice_id);

CREATE TYPE payment_attempt_status AS ENUM ('processing', 'succeeded', 'failed');

CREATE TABLE payment_attempts (
    id UUID PRIMARY KEY,
    invoice_id UUID NOT NULL REFERENCES invoices(id) ON DELETE CASCADE,
    status payment_attempt_status NOT NULL DEFAULT 'processing',
    failure_code TEXT,
    psp_ref UUID,
    idempotency_key TEXT NOT NULL,
    card_token_fingerprint TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_payment_attempts_invoice_id ON payment_attempts(invoice_id);

-- At most one succeeded payment per invoice
CREATE UNIQUE INDEX idx_payment_attempts_one_success
    ON payment_attempts(invoice_id)
    WHERE status = 'succeeded';

CREATE TABLE idempotency_keys (
    id UUID PRIMARY KEY,
    business_id UUID NOT NULL REFERENCES businesses(id) ON DELETE CASCADE,
    idempotency_key TEXT NOT NULL,
    request_hash TEXT NOT NULL,
    response_status INTEGER NOT NULL,
    response_body JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (business_id, idempotency_key)
);

CREATE TABLE webhook_endpoints (
    id UUID PRIMARY KEY,
    business_id UUID NOT NULL REFERENCES businesses(id) ON DELETE CASCADE,
    url TEXT NOT NULL,
    secret TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_webhook_endpoints_business_id ON webhook_endpoints(business_id);

CREATE TYPE webhook_event_status AS ENUM ('pending', 'delivered', 'dead');

CREATE TABLE webhook_events (
    id UUID PRIMARY KEY,
    business_id UUID NOT NULL REFERENCES businesses(id) ON DELETE CASCADE,
    event_type TEXT NOT NULL,
    payload JSONB NOT NULL,
    status webhook_event_status NOT NULL DEFAULT 'pending',
    attempt_count INTEGER NOT NULL DEFAULT 0,
    next_attempt_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_webhook_events_pending ON webhook_events(status, next_attempt_at)
    WHERE status = 'pending';

-- Demo business and API key are seeded at app startup (see ensure_demo_key in api_key.rs).
