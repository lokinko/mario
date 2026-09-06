-- Run this migration in the Supabase SQL editor for the project used by mario.
-- The service only receives ciphertext, nonce, hash and schema metadata.

create table if not exists public.sync_blobs (
  user_id uuid primary key references auth.users(id) on delete cascade,
  revision bigint not null check (revision > 0),
  ciphertext text not null,
  nonce text not null,
  content_hash text not null,
  schema_version integer not null check (schema_version > 0),
  updated_at timestamptz not null default now()
);

alter table public.sync_blobs enable row level security;

revoke all on table public.sync_blobs from anon;
revoke all on table public.sync_blobs from authenticated;
grant select on table public.sync_blobs to authenticated;

drop policy if exists "users can read only their encrypted blob" on public.sync_blobs;
create policy "users can read only their encrypted blob"
on public.sync_blobs
for select
to authenticated
using ((select auth.uid()) = user_id);

create or replace function public.upsert_sync_blob(
  p_expected_revision bigint,
  p_ciphertext text,
  p_nonce text,
  p_content_hash text,
  p_schema_version integer
)
returns bigint
language plpgsql
security definer
set search_path = ''
as $$
declare
  current_user_id uuid := (select auth.uid());
  next_revision bigint := p_expected_revision + 1;
  affected_rows integer;
begin
  if current_user_id is null then
    raise exception using errcode = '42501', message = 'authentication_required';
  end if;
  if p_expected_revision < 0 or p_schema_version < 1 then
    raise exception using errcode = '22023', message = 'invalid_sync_metadata';
  end if;
  if length(p_ciphertext) > 35000000 or length(p_nonce) > 128 or length(p_content_hash) > 128 then
    raise exception using errcode = '22023', message = 'sync_blob_too_large';
  end if;

  if p_expected_revision = 0 then
    insert into public.sync_blobs (
      user_id, revision, ciphertext, nonce, content_hash, schema_version, updated_at
    ) values (
      current_user_id, 1, p_ciphertext, p_nonce, p_content_hash, p_schema_version, now()
    ) on conflict (user_id) do nothing;
    get diagnostics affected_rows = row_count;
  else
    update public.sync_blobs
    set revision = next_revision,
        ciphertext = p_ciphertext,
        nonce = p_nonce,
        content_hash = p_content_hash,
        schema_version = p_schema_version,
        updated_at = now()
    where user_id = current_user_id and revision = p_expected_revision;
    get diagnostics affected_rows = row_count;
  end if;

  if affected_rows <> 1 then
    raise exception using errcode = '40001', message = 'sync_revision_conflict';
  end if;
  return next_revision;
end;
$$;

revoke all on function public.upsert_sync_blob(bigint, text, text, text, integer) from public;
revoke all on function public.upsert_sync_blob(bigint, text, text, text, integer) from anon;
grant execute on function public.upsert_sync_blob(bigint, text, text, text, integer) to authenticated;

