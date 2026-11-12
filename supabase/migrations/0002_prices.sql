-- Fase 7: actualización automática de precios.
-- Se ejecuta una vez en el SQL Editor, después de 0001_schema.sql.

-- Alertas de precio del usuario: se sincronizan como el resto de sus datos.
create table public.price_alerts (
  id uuid primary key,
  user_id uuid not null references auth.users (id) on delete cascade,
  updated_at text collate "C" not null,
  deleted_at text collate "C",
  server_seq bigint not null default 0,
  asset_id uuid not null,
  direction text not null check (direction in ('above', 'below')),
  target_price public.amount_value not null check (target_price >= 0),
  active boolean not null default true,
  note text
);

alter table public.price_alerts enable row level security;
create policy price_alerts_select on public.price_alerts for select to authenticated
  using (user_id = (select auth.uid()));
create policy price_alerts_insert on public.price_alerts for insert to authenticated
  with check (user_id = (select auth.uid()));
create policy price_alerts_update on public.price_alerts for update to authenticated
  using (user_id = (select auth.uid()))
  with check (user_id = (select auth.uid()));
create trigger sync_guard before insert or update on public.price_alerts
  for each row execute function private.sync_guard();
create index price_alerts_sync_idx on public.price_alerts (user_id, server_seq);
revoke all on public.price_alerts from public, anon, authenticated;
grant select, insert, update on public.price_alerts to authenticated;
grant all on public.price_alerts to service_role;

-- Registro de cada ejecución del worker. Solo lo lee y escribe el worker.
create table public.price_runs (
  id bigint generated always as identity primary key,
  started_at timestamptz not null,
  finished_at timestamptz not null default now(),
  ok boolean not null,
  symbols integer not null default 0,
  prices integer not null default 0,
  fx_rates integer not null default 0,
  errors jsonb not null default '[]'
);
alter table public.price_runs enable row level security;
revoke all on public.price_runs from public, anon, authenticated;
grant all on public.price_runs to service_role;

-- Símbolos en poder de algún usuario, sin repetir y sin decir de quién son.
create function public.held_symbols()
returns table (provider text, symbol text, currency text)
language sql
stable
security definer
set search_path = ''
as $$
  select distinct on (a.provider, upper(a.provider_symbol))
    a.provider, a.provider_symbol, a.currency::text
  from public.assets a
  where a.deleted_at is null
    and a.provider in ('eodhd', 'coingecko')
    and coalesce(a.provider_symbol, '') <> ''
  order by a.provider, upper(a.provider_symbol), a.currency
$$;

-- Monedas que usa alguien, para pedir solo esos tipos de cambio.
create function public.currencies_in_use()
returns table (currency text)
language sql
stable
security definer
set search_path = ''
as $$
  select base_currency::text from public.profiles where deleted_at is null
  union select currency::text from public.accounts where deleted_at is null
  union select currency::text from public.assets where deleted_at is null
  union select currency::text from public.transactions where deleted_at is null
  union select currency::text from public.debts where deleted_at is null
  union select currency::text from public.budgets where deleted_at is null
  union select currency::text from public.goals where deleted_at is null
$$;

-- Primera fecha con precio de cada símbolo, para saber cuáles necesitan historial.
create function public.price_coverage()
returns table (provider text, symbol text, first_on date, last_on date)
language sql
stable
security definer
set search_path = ''
as $$
  select provider, symbol, min(price_on), max(price_on)
  from public.asset_prices
  group by provider, symbol
$$;

create function public.fx_coverage()
returns table (base text, quote text, first_on date, last_on date)
language sql
stable
security definer
set search_path = ''
as $$
  select base::text, quote::text, min(rate_on), max(rate_on)
  from public.fx_rates
  group by base, quote
$$;

revoke all on function public.held_symbols() from public, anon, authenticated;
revoke all on function public.currencies_in_use() from public, anon, authenticated;
revoke all on function public.price_coverage() from public, anon, authenticated;
revoke all on function public.fx_coverage() from public, anon, authenticated;
grant execute on function public.held_symbols() to service_role;
grant execute on function public.currencies_in_use() to service_role;
grant execute on function public.price_coverage() to service_role;
grant execute on function public.fx_coverage() to service_role;

-- Los clientes buscan los precios de sus símbolos desde una fecha.
create index if not exists asset_prices_symbol_idx on public.asset_prices (symbol, price_on);
