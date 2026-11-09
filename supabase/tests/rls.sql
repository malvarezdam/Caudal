-- Prueba de seguridad entre usuarios. Se ejecuta en el SQL Editor.
-- No deja datos: todo se revierte al final.
-- Correcto: "Success. No rows returned". Cualquier fallo aparece como error.

begin;

do $$
declare
  a constant uuid := '00000000-0000-4000-8000-00000000000a';
  b constant uuid := '00000000-0000-4000-8000-00000000000b';
  x constant uuid := '00000000-0000-4000-8000-0000000000aa';
  claims_a constant text := '{"sub":"00000000-0000-4000-8000-00000000000a","role":"authenticated"}';
  claims_b constant text := '{"sub":"00000000-0000-4000-8000-00000000000b","role":"authenticated"}';
  user_tables constant text[] := array[
    'profiles', 'accounts', 'categories', 'tags', 'transactions',
    'transaction_splits', 'contacts', 'debts', 'debt_installments',
    'debt_payments', 'recurring_rules', 'budgets', 'goals',
    'goal_contributions', 'assets', 'trades'
  ];
  shared_tables constant text[] := array['asset_prices', 'fx_rates'];
  t text;
  n bigint;
  seq_before bigint;
  seq_after bigint;
  row_data jsonb;
begin
  -- Ninguna tabla pública puede quedar sin RLS.
  select count(*) into n from pg_tables
    where schemaname = 'public' and not rowsecurity;
  if n <> 0 then raise exception 'FALLO: % tablas sin RLS', n; end if;

  -- Toda tabla de usuario debe estar en esta prueba.
  select count(*) into n from information_schema.columns
    where table_schema = 'public' and column_name = 'user_id'
      and table_name <> all (user_tables);
  if n <> 0 then raise exception 'FALLO: % tablas de usuario sin probar', n; end if;

  insert into auth.users (id, email) values
    (a, 'caudal-rls-a@example.invalid'), (b, 'caudal-rls-b@example.invalid');

  insert into public.profiles (id, user_id, updated_at) values (a, a, '0001');
  insert into public.accounts (id, user_id, updated_at, name, currency) values (x, a, '0001', 'c', 'USD');
  insert into public.categories (id, user_id, updated_at, name) values (x, a, '0001', 'c');
  insert into public.tags (id, user_id, updated_at, name) values (x, a, '0001', 't');
  insert into public.transactions (id, user_id, updated_at, amount, currency, occurred_on) values (x, a, '0001', 1, 'USD', '2026-01-01');
  insert into public.transaction_splits (id, user_id, updated_at, transaction_id, amount) values (x, a, '0001', x, 1);
  insert into public.contacts (id, user_id, updated_at, name) values (x, a, '0001', 'c');
  insert into public.debts (id, user_id, updated_at, direction, principal, currency) values (x, a, '0001', 'i_owe', 1, 'USD');
  insert into public.debt_installments (id, user_id, updated_at, debt_id, due_on, amount) values (x, a, '0001', x, '2026-01-01', 1);
  insert into public.debt_payments (id, user_id, updated_at, debt_id, amount, paid_on) values (x, a, '0001', x, 1, '2026-01-01');
  insert into public.recurring_rules (id, user_id, updated_at, frequency, starts_on, amount, currency) values (x, a, '0001', 'monthly', '2026-01-01', 1, 'USD');
  insert into public.budgets (id, user_id, updated_at, amount, currency) values (x, a, '0001', 1, 'USD');
  insert into public.goals (id, user_id, updated_at, name, target_amount, currency) values (x, a, '0001', 'g', 1, 'USD');
  insert into public.goal_contributions (id, user_id, updated_at, goal_id, amount, contributed_on) values (x, a, '0001', x, 1, '2026-01-01');
  insert into public.assets (id, user_id, updated_at, name, currency) values (x, a, '0001', 'a', 'USD');
  insert into public.trades (id, user_id, updated_at, asset_id, kind, traded_on, currency) values (x, a, '0001', x, 'buy', '2026-01-01', 'USD');

  foreach t in array user_tables loop
    execute format('select to_jsonb(r) from public.%I r where user_id = $1', t)
      into row_data using a;
    if row_data is null then raise exception 'FALLO: % sin fila de prueba', t; end if;
    row_data := jsonb_set(row_data, '{id}', to_jsonb(gen_random_uuid()));

    -- Otro usuario no ve, no cambia, no borra y no suplanta.
    perform set_config('request.jwt.claims', claims_b, true);
    perform set_config('role', 'authenticated', true);

    execute format('select count(*) from public.%I', t) into n;
    if n <> 0 then raise exception 'FALLO: B ve filas de A en %', t; end if;

    execute format('update public.%I set updated_at = ''9999''', t);
    get diagnostics n = row_count;
    if n <> 0 then raise exception 'FALLO: B modifica filas de A en %', t; end if;

    begin
      execute format('delete from public.%I', t);
      raise exception 'FALLO: DELETE permitido en %', t using errcode = 'P0001';
    exception when insufficient_privilege then null;
    end;

    begin
      execute format(
        'insert into public.%I select * from jsonb_populate_record(null::public.%I, $1)', t, t
      ) using row_data;
      raise exception 'FALLO: B inserta como A en %', t using errcode = 'P0001';
    exception when insufficient_privilege then null;
    end;

    -- Sin sesión no hay acceso.
    perform set_config('request.jwt.claims', '', true);
    perform set_config('role', 'anon', true);
    begin
      execute format('select count(*) from public.%I', t) into n;
      raise exception 'FALLO: anon lee %', t using errcode = 'P0001';
    exception when insufficient_privilege then null;
    end;

    -- El dueño ve, actualiza y no regala sus filas.
    perform set_config('request.jwt.claims', claims_a, true);
    perform set_config('role', 'authenticated', true);

    execute format('select count(*) from public.%I', t) into n;
    if n <> 1 then raise exception 'FALLO: A no ve su fila en %', t; end if;

    execute format('select server_seq from public.%I', t) into seq_before;

    execute format('update public.%I set updated_at = ''0002''', t);
    get diagnostics n = row_count;
    if n <> 1 then raise exception 'FALLO: A no actualiza %', t; end if;

    execute format('select server_seq from public.%I', t) into seq_after;
    if seq_after <= seq_before then
      raise exception 'FALLO: cursor sin avanzar en %', t;
    end if;

    -- Gana el último cambio: lo antiguo o repetido se ignora.
    execute format('update public.%I set updated_at = ''0001''', t);
    get diagnostics n = row_count;
    if n <> 0 then raise exception 'FALLO: cambio antiguo aceptado en %', t; end if;
    execute format('update public.%I set updated_at = ''0002''', t);
    get diagnostics n = row_count;
    if n <> 0 then raise exception 'FALLO: cambio repetido aceptado en %', t; end if;

    begin
      execute format('update public.%I set updated_at = ''0003'', user_id = $1', t) using b;
      raise exception 'FALLO: A entrega su fila en %', t using errcode = 'P0001';
    exception when insufficient_privilege then null;
    end;

    perform set_config('role', 'none', true);
    perform set_config('request.jwt.claims', '', true);
  end loop;

  -- Datos de mercado: lectura con sesión, escritura solo del worker.
  foreach t in array shared_tables loop
    perform set_config('request.jwt.claims', claims_a, true);
    perform set_config('role', 'authenticated', true);

    execute format('select count(*) from public.%I', t) into n;

    begin
      execute format('delete from public.%I', t);
      raise exception 'FALLO: DELETE permitido en %', t using errcode = 'P0001';
    exception when insufficient_privilege then null;
    end;

    begin
      if t = 'fx_rates' then
        insert into public.fx_rates (base, quote, rate_on, rate)
          values ('USD', 'EUR', '2026-01-01', 1);
      else
        insert into public.asset_prices (provider, symbol, price_on, price, currency)
          values ('eodhd', 'X', '2026-01-01', 1, 'USD');
      end if;
      raise exception 'FALLO: usuario escribe en %', t using errcode = 'P0001';
    exception when insufficient_privilege then null;
    end;

    perform set_config('request.jwt.claims', '', true);
    perform set_config('role', 'anon', true);
    begin
      execute format('select count(*) from public.%I', t) into n;
      raise exception 'FALLO: anon lee %', t using errcode = 'P0001';
    exception when insufficient_privilege then null;
    end;

    perform set_config('role', 'none', true);
  end loop;

  -- Reglas del dominio.
  begin
    insert into public.transactions (id, user_id, updated_at, amount, currency, occurred_on)
      values (gen_random_uuid(), a, '0001', -1, 'USD', '2026-01-01');
    raise exception 'FALLO: gasto negativo aceptado' using errcode = 'P0001';
  exception when check_violation then null;
  end;

  insert into public.transactions (id, user_id, updated_at, kind, amount, currency, occurred_on)
    values (gen_random_uuid(), a, '0001', 'adjustment', -1, 'USD', '2026-01-01');

  begin
    insert into public.accounts (id, user_id, updated_at, name, currency)
      values (gen_random_uuid(), a, '0001', 'c', 'usd');
    raise exception 'FALLO: moneda inválida aceptada' using errcode = 'P0001';
  exception when check_violation then null;
  end;

  -- Eliminar la cuenta borra todos sus datos.
  delete from auth.users where id = a;
  foreach t in array user_tables loop
    execute format('select count(*) from public.%I where user_id = $1', t) into n using a;
    if n <> 0 then raise exception 'FALLO: quedan datos en % tras eliminar la cuenta', t; end if;
  end loop;
end;
$$;

rollback;
