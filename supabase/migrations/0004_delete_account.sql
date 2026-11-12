-- Fase 9: eliminación total de la cuenta por el propio usuario.
-- Se ejecuta una vez en el SQL Editor, después de 0003_data.sql.
--
-- Los archivos de Storage (`receipts/{user_id}/`) los borra antes el cliente con
-- la API de Storage, porque Supabase no permite borrarlos directamente por SQL.
-- Al borrar la fila de `auth.users`, todas las tablas `public.*` del usuario se
-- eliminan en cascada (`on delete cascade`).

create function public.delete_account()
returns void
language plpgsql
security definer
set search_path = ''
as $$
declare
  uid uuid := (select auth.uid());
begin
  if uid is null then
    raise exception 'no hay sesión iniciada' using errcode = '42501';
  end if;
  delete from auth.users where id = uid;
end;
$$;

revoke all on function public.delete_account() from public, anon;
grant execute on function public.delete_account() to authenticated;
