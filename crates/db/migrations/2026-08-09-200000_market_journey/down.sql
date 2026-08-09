ALTER TABLE markets
    DROP COLUMN IF EXISTS start_eta_unix,
    DROP COLUMN IF EXISTS start_stop_id;
