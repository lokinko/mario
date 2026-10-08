CREATE TABLE IF NOT EXISTS data_revision (
    id INTEGER PRIMARY KEY CHECK (id=1), revision BIGINT NOT NULL DEFAULT 0
);
INSERT INTO data_revision(id,revision) VALUES (1,0) ON CONFLICT DO NOTHING;
CREATE OR REPLACE FUNCTION bump_data_revision() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    UPDATE data_revision SET revision=revision+1 WHERE id=1;
    RETURN NULL;
END;
$$;
DO $$
DECLARE item record;
BEGIN
    FOR item IN SELECT tablename FROM pg_tables WHERE schemaname=current_schema() AND tablename <> 'data_revision'
    LOOP
        IF NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgrelid=format('%I.%I',current_schema(),item.tablename)::regclass AND tgname='data_changed') THEN
            EXECUTE format('CREATE TRIGGER data_changed AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION bump_data_revision()',item.tablename);
        END IF;
    END LOOP;
END;
$$;
