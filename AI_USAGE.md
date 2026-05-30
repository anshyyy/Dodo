# AI Usage Disclosure

## Tools used

- **Cursor (Claude)** — Primary implementation assistant for this repository: project scaffolding, Axum route wiring, SQLx repository layer, payment/idempotency flow, docker-compose, integration tests, and first drafts of `DESIGN.md`, `README.md`, and `openapi.yaml`.

## Three decisions made independently (or against AI defaults)

1. **Partial unique index on `processing` payment attempts** — AI initially suggested only serializing on `succeeded`. I added a second partial unique index so concurrent pays fail fast with `payment_in_progress` instead of allowing multiple in-flight PSP calls.

2. **Same-process mock PSP over HTTP** — Per assignment preference, mock PSP lives at `/mock-psp`, but the invoice service still calls it via `reqwest` and `MOCK_PSP_BASE_URL` so the pay path matches a real external dependency.

3. **202 + background task for slow PSP** — Rather than blocking the handler for 30s or failing `tok_timeout` immediately, the sync wait budget returns 202 while a detached task completes the PSP call with a longer client timeout.

## Something corrected

The first payment success path updated `payment_attempts` outside the open transaction (pool vs transaction mismatch). I moved those updates inside the same transaction as the invoice state transition before commit.

## Verification

- `cargo build` on the full workspace.
- Integration tests (with Postgres): concurrent pay, idempotency PSP call count, network-error token — documented in `README.md`.
- Manual review of state transitions and idempotency hash behavior against assignment section 3.

If you did not use AI for the demo video or final DESIGN edits, note that here before submission.
