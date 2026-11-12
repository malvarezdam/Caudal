# Caudal Privacy Policy

Service controller: **[RESPONSABLE]**
Contact: **[CORREO DE CONTACTO]**

Caudal is a personal finance application for Windows, Android and the web. This policy explains what data is stored, where it lives and how you can export or delete it at any time.

## 1. What data is stored

- **Account:** your email address, your unique account identifier and your hashed password (managed by Supabase Auth; Caudal never sees or stores your plain-text password).
- **Financial data you enter:** profile (optional display name, main currency and language), accounts, transactions, categories, tags, contacts, debts, installments and payments, recurring rules, budgets, savings goals, assets, investment trades, price alerts, auto-categorization rules and templates.
- **Receipts:** images (JPG, PNG, WEBP, HEIC) or PDF files you attach to a transaction are stored in a private folder linked solely to your account (`receipts/{user_id}/`) in Supabase Storage.
- **On your device:** a local copy of your data (in SQLite on Windows and Android, or in IndexedDB in the browser) so the app works offline. Your lock PIN (stored only as a salted hash) and private mode stay on that device only and are never sent to the server.

## 2. How your data is used

- Solely to display your financial information and sync it across your own devices when you are online.
- **We do not sell, rent or share your personal or financial data with third parties** for advertising or commercial purposes.
- Every table and storage folder enforces Row Level Security (RLS) in the database so no other account can read or modify your records.

## 3. Third-party prices and exchange rates

- To value your portfolio and convert currencies, an automated server worker fetches public market prices for stocks and ETFs (EODHD), cryptocurrencies (CoinGecko) and exchange rates (Frankfurter / European Central Bank and `currency-api`).
- Those providers only receive queries for the market symbols in use (for example, `AAPL.US` or `bitcoin`), **never your identity, email or quantities**. Your device only reads the public `asset_prices` and `fx_rates` tables.

## 4. How to export and delete your data

You have full control over your information from **Settings → Your data**:

- **Export:** you can download all your transactions as CSV or a **full JSON backup** containing every table (including the trash).
- **Delete data:** you can move individual items to the trash, use **Delete all my data** to clear your records while keeping your account, or use **Delete my account** to permanently delete your receipts in Supabase Storage, all your database rows, your authentication user and the local data on your device.
- You can also contact **[CORREO DE CONTACTO]** with any question or request regarding your data.
