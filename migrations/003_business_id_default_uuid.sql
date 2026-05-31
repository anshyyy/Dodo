-- Random UUID default + replace placeholder demo id (no-op if schema not ready).
DO $$
DECLARE
    old_id UUID := '11111111-1111-1111-1111-111111111111';
    new_id UUID := gen_random_uuid();
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_schema = 'public' AND table_name = 'businesses'
    ) THEN
        RETURN;
    END IF;

    -- Idempotent for DBs created before 001 had DEFAULT on businesses.id
    ALTER TABLE businesses
        ALTER COLUMN id SET DEFAULT gen_random_uuid();

    IF EXISTS (SELECT 1 FROM businesses WHERE id = old_id) THEN
        UPDATE api_keys SET business_id = new_id WHERE business_id = old_id;
        UPDATE customers SET business_id = new_id WHERE business_id = old_id;
        UPDATE invoices SET business_id = new_id WHERE business_id = old_id;
        UPDATE idempotency_keys SET business_id = new_id WHERE business_id = old_id;
        UPDATE webhook_endpoints SET business_id = new_id WHERE business_id = old_id;
        UPDATE webhook_events SET business_id = new_id WHERE business_id = old_id;
        UPDATE businesses SET id = new_id WHERE id = old_id;
    END IF;
END $$;
