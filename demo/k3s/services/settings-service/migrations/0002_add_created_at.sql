-- 0002_add_created_at.sql
-- Backward-compatible additive migration (be-arch §11): adds a NEW nullable
-- column so the forward path never breaks existing rows. `version` and the
-- unique identity index already exist from 0001; seeding rows before this
-- migration and re-applying the forward path must leave them readable with
-- their existing `version` intact and the new column at its default (`NULL`).
ALTER TABLE settings ADD COLUMN created_at BIGINT;
