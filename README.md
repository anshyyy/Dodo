# Dodo Invoice & Payment Service

Minimal invoice and payment API (Rust, Axum, PostgreSQL) with mock PSP at `/mock-psp/v1/charges` and async signed webhooks.

## Quick start

```bash
docker compose up --build
```

- API: `http://localhost:8080`
- **Swagger UI:** `http://localhost:8080/docs`
- OpenAPI spec: `http://localhost:8080/api/openapi.yaml`
- Health: `GET /health`
- Mock PSP: `POST http://localhost:8080/mock-psp/v1/charges`

Demo API key (seeded on first run):

```text
dodo_test_key_demo12345678901234567890
```

Send as header: `X-Api-Key: <key>` or `Authorization: Bearer <key>`.

## Example requests

Create a customer:

```bash
curl -s -X POST http://localhost:8080/api/v1/customers \
  -H "X-Api-Key: dodo_test_key_demo12345678901234567890" \
  -H "Content-Type: application/json" \
  -d '{"name":"Jane Doe","email":"jane@example.com"}'
```

Create an invoice (total computed server-side from line items):

```bash
curl -s -X POST http://localhost:8080/api/v1/invoices \
  -H "X-Api-Key: dodo_test_key_demo12345678901234567890" \
  -H "Content-Type: application/json" \
  -d '{
    "customer_id": "<CUSTOMER_UUID>",
    "due_date": "2026-12-31",
    "line_items": [
      {"description": "Consulting", "quantity": 2, "unit_amount_cents": 5000}
    ]
  }'
```

Pay successfully:

```bash
curl -s -X POST "http://localhost:8080/api/v1/invoices/<INVOICE_UUID>/pay" \
  -H "X-Api-Key: dodo_test_key_demo12345678901234567890" \
  -H "Idempotency-Key: pay-001" \
  -H "Content-Type: application/json" \
  -d '{"card_token":"tok_success"}'
```

Pay declined:

```bash
curl -s -X POST "http://localhost:8080/api/v1/invoices/<INVOICE_UUID>/pay" \
  -H "X-Api-Key: dodo_test_key_demo12345678901234567890" \
  -H "Idempotency-Key: pay-002" \
  -H "Content-Type: application/json" \
  -d '{"card_token":"tok_card_declined"}'
```

Mock card tokens: `tok_success`, `tok_insufficient_funds`, `tok_card_declined`, `tok_timeout` (30s PSP delay — API returns 202 if still processing), `tok_network_error`.

## Tests

Start Postgres, then run integration tests:

```bash
docker compose up -d postgres
DATABASE_URL=postgres://dodo:dodo@localhost:5433/dodo cargo test --test integration_tests
```

Required tests: concurrent pay, idempotency replay, PSP network failure.

## Demo Video

<!-- Replace with your Loom / Drive link before submission -->
https://example.com/your-demo-video

## Documentation

- [DESIGN.md](./DESIGN.md) — data model, state machine, failure modes
- [AI_USAGE.md](./AI_USAGE.md) — AI tool disclosure
- [openapi.yaml](./openapi.yaml) — HTTP API reference

## Local development (without Docker app image)

```bash
docker compose up -d postgres
cp .env.example .env   # optional; app also reads env vars with defaults in config.rs
cargo run
```

Environment variables (see [`.env.example`](.env.example)):

| Variable | Purpose |
|----------|---------|
| `DATABASE_URL` | PostgreSQL connection string |
| `LISTEN_ADDR` | Bind address (default `0.0.0.0:8080`) |
| `MOCK_PSP_BASE_URL` | Base URL for HTTP PSP calls (default same server) |
| `PAY_SYNC_WAIT_SECS` | Max wait on `/pay` before returning 202 |
| `PSP_HTTP_TIMEOUT_SECS` | Background PSP HTTP client timeout |
| `RUST_LOG` | Tracing filter |

**Note:** `.env` is in `.gitignore` so secrets are not committed. Use `.env.example` as the template.
