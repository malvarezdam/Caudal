-- Caudal: esquema inicial.
--
-- Las tablas de usuario no tienen claves foráneas entre sí: la sincronización
-- offline puede subir las filas en cualquier orden. Solo user_id referencia a
-- auth.users, para que eliminar una cuenta borre todos sus datos.

create schema if not exists private;

create domain public.currency_code as text
  check (value ~ '^[A-Z0-9]{2,10}$');

-- 16 enteros y 12 decimales: cabe siempre en rust_decimal.
create domain public.amount_value as numeric(28, 12);

create sequence private.sync_seq;

-- Aplica "gana el último cambio" en el servidor y asigna el cursor de descarga.
create function private.sync_guard()
returns trigger
language plpgsql
security definer
set search_path = ''
as $$
begin
  if tg_op = 'UPDATE' and new.updated_at <= old.updated_at then
    return null;
  end if;
  new.server_seq := nextval('private.sync_seq');
  return new;
end;
$$;

create table public.profiles (
  id uuid primary key,
  user_id uuid not null unique references auth.users (id) on delete cascade,
  updated_at text collate "C" not null,
  deleted_at text collate "C",
  server_seq bigint not null default 0,
  base_currency public.currency_code not null default 'USD',
  locale text,
  display_name text,
  check (id = user_id)
);

create table public.accounts (
  id uuid primary key,
  user_id uuid not null references auth.users (id) on delete cascade,
  updated_at text collate "C" not null,
  deleted_at text collate "C",
  server_seq bigint not null default 0,
  name text not null,
  kind text not null default 'other'
    check (kind in ('cash', 'bank', 'card', 'wallet', 'broker', 'other')),
  currency public.currency_code not null,
  opening_balance public.amount_value not null default 0,
  archived boolean not null default false,
  color text,
  icon text,
  note text
);

create table public.categories (
  id uuid primary key,
  user_id uuid not null references auth.users (id) on delete cascade,
  updated_at text collate "C" not null,
  deleted_at text collate "C",
  server_seq bigint not null default 0,
  name text not null,
  parent_id uuid,
  kind text not null default 'both'
    check (kind in ('income', 'expense', 'both')),
  color text,
  icon text
);

create table public.tags (
  id uuid primary key,
  user_id uuid not null references auth.users (id) on delete cascade,
  updated_at text collate "C" not null,
  deleted_at text collate "C",
  server_seq bigint not null default 0,
  name text not null,
  color text
);

create table public.transactions (
  id uuid primary key,
  user_id uuid not null references auth.users (id) on delete cascade,
  updated_at text collate "C" not null,
  deleted_at text collate "C",
  server_seq bigint not null default 0,
  kind text not null default 'expense'
    check (kind in ('income', 'expense', 'transfer', 'adjustment')),
  status text not null default 'confirmed'
    check (status in ('confirmed', 'pending')),
  amount public.amount_value not null,
  currency public.currency_code not null,
  occurred_on date not null,
  account_id uuid,
  to_account_id uuid,
  account_amount public.amount_value,
  to_amount public.amount_value,
  base_amount public.amount_value,
  base_currency public.currency_code,
  category_id uuid,
  tag_ids uuid[] not null default '{}',
  contact_id uuid,
  recurring_rule_id uuid,
  occurrence_date date,
  description text,
  note text,
  check (kind = 'adjustment' or amount >= 0)
);

create table public.transaction_splits (
  id uuid primary key,
  user_id uuid not null references auth.users (id) on delete cascade,
  updated_at text collate "C" not null,
  deleted_at text collate "C",
  server_seq bigint not null default 0,
  transaction_id uuid not null,
  category_id uuid,
  amount public.amount_value not null,
  note text
);

create table public.contacts (
  id uuid primary key,
  user_id uuid not null references auth.users (id) on delete cascade,
  updated_at text collate "C" not null,
  deleted_at text collate "C",
  server_seq bigint not null default 0,
  name text not null,
  phone text,
  note text
);

create table public.debts (
  id uuid primary key,
  user_id uuid not null references auth.users (id) on delete cascade,
  updated_at text collate "C" not null,
  deleted_at text collate "C",
  server_seq bigint not null default 0,
  contact_id uuid,
  direction text not null check (direction in ('owed_to_me', 'i_owe')),
  principal public.amount_value not null check (principal >= 0),
  currency public.currency_code not null,
  interest_rate numeric(12, 6),
  started_on date,
  due_on date,
  closed boolean not null default false,
  description text
);

create table public.debt_installments (
  id uuid primary key,
  user_id uuid not null references auth.users (id) on delete cascade,
  updated_at text collate "C" not null,
  deleted_at text collate "C",
  server_seq bigint not null default 0,
  debt_id uuid not null,
  due_on date not null,
  amount public.amount_value not null check (amount >= 0),
  note text
);

create table public.debt_payments (
  id uuid primary key,
  user_id uuid not null references auth.users (id) on delete cascade,
  updated_at text collate "C" not null,
  deleted_at text collate "C",
  server_seq bigint not null default 0,
  debt_id uuid not null,
  installment_id uuid,
  transaction_id uuid,
  amount public.amount_value not null check (amount >= 0),
  paid_on date not null,
  note text
);

create table public.recurring_rules (
  id uuid primary key,
  user_id uuid not null references auth.users (id) on delete cascade,
  updated_at text collate "C" not null,
  deleted_at text collate "C",
  server_seq bigint not null default 0,
  name text,
  frequency text not null
    check (frequency in ('daily', 'weekly', 'monthly', 'yearly')),
  interval_count integer not null default 1 check (interval_count > 0),
  last_day_of_month boolean not null default false,
  starts_on date not null,
  ends_on date,
  mode text not null default 'confirm' check (mode in ('auto', 'confirm')),
  active boolean not null default true,
  kind text not null default 'expense'
    check (kind in ('income', 'expense', 'transfer', 'adjustment')),
  amount public.amount_value not null,
  currency public.currency_code not null,
  account_id uuid,
  to_account_id uuid,
  category_id uuid,
  description text
);

create table public.budgets (
  id uuid primary key,
  user_id uuid not null references auth.users (id) on delete cascade,
  updated_at text collate "C" not null,
  deleted_at text collate "C",
  server_seq bigint not null default 0,
  category_id uuid,
  amount public.amount_value not null check (amount >= 0),
  currency public.currency_code not null,
  period text not null default 'monthly' check (period in ('monthly', 'yearly')),
  starts_on date,
  ends_on date,
  rollover boolean not null default false
);

create table public.goals (
  id uuid primary key,
  user_id uuid not null references auth.users (id) on delete cascade,
  updated_at text collate "C" not null,
  deleted_at text collate "C",
  server_seq bigint not null default 0,
  name text not null,
  target_amount public.amount_value not null check (target_amount >= 0),
  currency public.currency_code not null,
  target_date date,
  account_id uuid,
  note text
);

create table public.goal_contributions (
  id uuid primary key,
  user_id uuid not null references auth.users (id) on delete cascade,
  updated_at text collate "C" not null,
  deleted_at text collate "C",
  server_seq bigint not null default 0,
  goal_id uuid not null,
  transaction_id uuid,
  amount public.amount_value not null,
  contributed_on date not null,
  note text
);

create table public.assets (
  id uuid primary key,
  user_id uuid not null references auth.users (id) on delete cascade,
  updated_at text collate "C" not null,
  deleted_at text collate "C",
  server_seq bigint not null default 0,
  name text not null,
  kind text not null default 'other'
    check (kind in ('stock', 'etf', 'crypto', 'fund', 'bond', 'real_estate', 'other')),
  currency public.currency_code not null,
  provider text not null default 'manual'
    check (provider in ('eodhd', 'coingecko', 'manual')),
  provider_symbol text,
  manual_price public.amount_value check (manual_price >= 0),
  manual_price_on date,
  note text
);

create table public.trades (
  id uuid primary key,
  user_id uuid not null references auth.users (id) on delete cascade,
  updated_at text collate "C" not null,
  deleted_at text collate "C",
  server_seq bigint not null default 0,
  asset_id uuid not null,
  kind text not null
    check (kind in ('buy', 'sell', 'dividend', 'fee', 'split', 'transfer_in', 'transfer_out')),
  traded_on date not null,
  quantity public.amount_value check (quantity >= 0),
  price public.amount_value check (price >= 0),
  fee public.amount_value check (fee >= 0),
  currency public.currency_code not null,
  split_ratio public.amount_value check (split_ratio > 0),
  account_id uuid,
  transaction_id uuid,
  note text
);

-- Datos de mercado comunes: los usuarios leen y solo el worker escribe.
create table public.asset_prices (
  provider text not null check (provider in ('eodhd', 'coingecko')),
  symbol text not null,
  price_on date not null,
  price public.amount_value not null check (price >= 0),
  currency public.currency_code not null,
  fetched_at timestamptz not null default now(),
  primary key (provider, symbol, price_on)
);

create table public.fx_rates (
  base public.currency_code not null,
  quote public.currency_code not null,
  rate_on date not null,
  rate public.amount_value not null check (rate > 0),
  primary key (base, quote, rate_on)
);

-- Seguridad, cursor y permisos de todas las tablas de usuario.
do $$
declare
  t text;
begin
  foreach t in array array[
    'profiles', 'accounts', 'categories', 'tags', 'transactions',
    'transaction_splits', 'contacts', 'debts', 'debt_installments',
    'debt_payments', 'recurring_rules', 'budgets', 'goals',
    'goal_contributions', 'assets', 'trades'
  ] loop
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
    -- Sin DELETE: los clientes solo hacen borrado lógico.
    execute format('revoke all on public.%I from public, anon, authenticated', t);
    execute format('grant select, insert, update on public.%I to authenticated', t);
    execute format('grant all on public.%I to service_role', t);
  end loop;

  foreach t in array array['asset_prices', 'fx_rates'] loop
    execute format('alter table public.%I enable row level security', t);
    execute format(
      'create policy %I on public.%I for select to authenticated using (true)',
      t || '_select', t);
    execute format('revoke all on public.%I from public, anon, authenticated', t);
    execute format('grant select on public.%I to authenticated', t);
    execute format('grant all on public.%I to service_role', t);
  end loop;
end;
$$;

revoke all on schema private from public, anon, authenticated;
