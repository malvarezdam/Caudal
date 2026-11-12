# Política de privacidad de Caudal

Responsable del servicio: **[RESPONSABLE]**
Contacto: **[CORREO DE CONTACTO]**

Caudal es una aplicación de finanzas personales para Windows, Android y web. Esta política explica qué datos se guardan, dónde viven y cómo puedes exportarlos o borrarlos en cualquier momento.

## 1. Qué datos se guardan

- **Cuenta:** tu dirección de correo electrónico, el identificador único de tu cuenta y tu contraseña cifrada (gestionada por Supabase Auth; Caudal nunca ve ni guarda tu contraseña en claro).
- **Datos financieros que tú registras:** perfil (nombre opcional, moneda principal e idioma), cuentas, movimientos, categorías, etiquetas, contactos, deudas, cuotas y pagos, reglas periódicas, presupuestos, metas de ahorro, activos, operaciones de inversión, alertas de precio, reglas de autocategorización y plantillas.
- **Comprobantes:** las imágenes (JPG, PNG, WEBP, HEIC) o archivos PDF que adjuntes a un movimiento se guardan en una carpeta privada asociada únicamente a tu cuenta (`receipts/{user_id}/`) en Supabase Storage.
- **En tu dispositivo:** una copia local de tus datos (en SQLite en Windows y Android, o en IndexedDB en el navegador) para que la aplicación funcione sin conexión a internet. El PIN de bloqueo (guardado solo como hash con sal) y el modo privado se guardan únicamente en ese dispositivo y nunca se envían al servidor.

## 2. Para qué se usan tus datos

- Exclusivamente para mostrarte tu información financiera y sincronizarla entre tus propios dispositivos cuando tienes conexión a internet.
- **No vendemos, alquilamos ni compartimos tus datos personales ni financieros con terceros** con fines publicitarios ni comerciales.
- Cada tabla y cada carpeta de archivos tiene políticas de seguridad a nivel de fila (Row Level Security, RLS) en la base de datos para que ninguna otra cuenta pueda leer ni modificar tus registros.

## 3. Precios y tipos de cambio de terceros

- Para valorar tu cartera y convertir monedas, un proceso automático del servidor consulta precios públicos de acciones y ETF (EODHD), criptomonedas (CoinGecko) y tipos de cambio (Frankfurter / Banco Central Europeo y `currency-api`).
- Esos proveedores solo reciben la consulta de los símbolos de mercado en uso (por ejemplo, `AAPL.US` o `bitcoin`), **nunca tu identidad, tu correo ni tus cantidades**. Tu dispositivo solo lee las tablas públicas de precios (`asset_prices`) y tipos de cambio (`fx_rates`).

## 4. Cómo exportar y borrar tus datos

Tienes control total sobre tu información desde **Ajustes → Tus datos**:

- **Exportar:** puedes descargar todos tus movimientos en formato CSV o una **copia completa en JSON** con todas las tablas (incluida la papelera).
- **Borrar datos:** puedes enviar elementos individuales a la papelera, usar **Borrar todos mis datos** para vaciar tus registros conservando la cuenta, o usar **Eliminar mi cuenta** para borrar de forma definitiva tus comprobantes en Supabase Storage, todos tus registros en la base de datos, tu usuario de autenticación y los datos locales del dispositivo.
- También puedes escribir a **[CORREO DE CONTACTO]** para cualquier consulta o solicitud sobre tus datos.
