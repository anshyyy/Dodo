-- Only one in-flight processing attempt per invoice
CREATE UNIQUE INDEX idx_payment_attempts_one_processing
    ON payment_attempts(invoice_id)
    WHERE status = 'processing';
