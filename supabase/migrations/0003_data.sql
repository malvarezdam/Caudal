-- Fase 8: reglas de categoría, plantillas y comprobantes adjuntos.
-- Se ejecuta una vez en el SQL Editor, después de 0002_prices.sql.

create table public.category_rules (
  id uuid primary key,
  user_id uuid not null references auth.users (id) on delete cascade,
  updated_at text collate "C" not null,
  deleted_at text collate "C",
  server_seq bigint not null default 0,
  pattern text not null check (length(trim(pattern)) > 0),
  category_id uuid not null,
  tag_ids uuid[] not null default '{}',
  active boolean not null default true
);

create table public.transaction_templates (
  id uuid primary key,
  user_id uuid not null references auth.users (id) on delete cascade,
  updated_at text collate "C" not null,
  deleted_at text collate "C",
  server_seq bigint not null default 0,
  name text not null,
  kind text not null default 'expense'
    check (kind in ('income', 'expense', 'transfer', 'adjustment')),
  amount public.amount_value,
  currency public.currency_code not null,
  account_id uuid,
  to_account_id uuid,
  category_id uuid,
  tag_ids uuid[] not null default '{}',
  description text
);

create table public.attachments (
  id uuid primary key,
  user_id uuid not null references auth.users (id) on delete cascade,
  updated_at text collate "C" not null,
  deleted_at text collate "C",
  server_seq bigint not null default 0,
  transaction_id uuid not null,
  file_name text not null,
  mime_type text not null,
  size_bytes bigint not null check (size_bytes >= 0 and size_bytes <= 10485760),
  -- El archivo solo puede estar en la carpeta del propio usuario.
  storage_path text not null,
  constraint attachments_own_folder check (storage_path = user_id::text || '/' || id::text)
);

do $$
declare
  t text;
begin
  foreach t in array array['category_rules', 'transaction_templates', 'attachments'] loop
    execute format('alter table public.%I enable row level security', t);
    execute format(
      'create policy %I on public.%I for select to authenticated
         using (user_id = (select auth.uid()))',
      t || '_select', t);
    execute format(
      'create policy %I on public.%I for insert to authenticated
         with check (user_id = (select auth.uid()))',
      t || '_insert', t);
    execute format(
      'create policy %I on public.%I for update to authenticated
         using (user_id = (select auth.uid()))
         with check (user_id = (select auth.uid()))',
      t || '_update', t);
    execute format(
      'create trigger sync_guard before insert or update on public.%I
         for each row execute function private.sync_guard()', t);
    execute format(
      'create index %I on public.%I (user_id, server_seq)', t || '_sync_idx', t);
    execute format('revoke all on public.%I from public, anon, authenticated', t);
    execute format('grant select, insert, update on public.%I to authenticated', t);
    execute format('grant all on public.%I to service_role', t);
  end loop;
end;
$$;

-- Comprobantes: bucket privado, hasta 10 MB, solo imágenes y PDF.
insert into storage.buckets (id, name, public, file_size_limit, allowed_mime_types)
values (
  'receipts', 'receipts', false, 10485760,
  array['image/jpeg', 'image/png', 'image/webp', 'image/heic', 'application/pdf']
)
on conflict (id) do nothing;

-- Cada usuario solo ve y escribe en su carpeta `{user_id}/`.
create policy receipts_select on storage.objects for select to authenticated
  using (bucket_id = 'receipts'
         and (storage.foldername(name))[1] = (select auth.uid())::text);
create policy receipts_insert on storage.objects for insert to authenticated
  with check (bucket_id = 'receipts'
              and (storage.foldername(name))[1] = (select auth.uid())::text);
create policy receipts_update on storage.objects for update to authenticated
  using (bucket_id = 'receipts'
         and (storage.foldername(name))[1] = (select auth.uid())::text)
  with check (bucket_id = 'receipts'
              and (storage.foldername(name))[1] = (select auth.uid())::text);
create policy receipts_delete on storage.objects for delete to authenticated
  using (bucket_id = 'receipts'
         and (storage.foldername(name))[1] = (select auth.uid())::text);
