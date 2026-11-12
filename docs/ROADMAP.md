# Roadmap: Caudal

Las decisiones y reglas técnicas están en [CLAUDE.md](../CLAUDE.md). Una fase solo se cierra cuando cumple su criterio de salida.

## Resultado final

Una persona crea su cuenta desde Windows, Android o la web y organiza todo su dinero en un solo lugar. Puede registrar un gasto sin internet y verlo en sus otros dispositivos al reconectarse.

Al abrir la app ve su **patrimonio neto**, el flujo del mes, el avance de presupuestos, los próximos vencimientos y el valor de sus inversiones con la fecha del último precio.

### Funciones completas

| Área | Qué incluye |
|---|---|
| Cuenta | Registro, login, recuperación de contraseña, PIN local, eliminación de cuenta |
| Cuentas de dinero | Efectivo, banco, tarjeta, billetera, broker; moneda propia; ajuste manual de saldo |
| Movimientos | Ingreso, gasto, transferencia, división en varias categorías, notas, adjuntos, plantillas rápidas |
| Clasificación | Categorías, subcategorías, etiquetas, reglas de autocategorización |
| Deudas | Quién te debe y a quién debes, cuotas, interés opcional, pagos parciales, vencimientos, historial por persona, gastos compartidos |
| Periódicos | Cualquier frecuencia, automático o por confirmar, vista de suscripciones |
| Planificación | Presupuestos, metas de ahorro, calendario financiero |
| Inversiones | Acciones, ETF, criptos, fondos y activos manuales; compras, ventas, dividendos, comisiones; precio promedio; ganancia realizada y no realizada |
| Precios | Actualización automática 1 o 2 veces al día de cada activo en cartera, precio manual, alertas de precio |
| Análisis | Resumen mensual y anual, comparativas, flujo de caja, gasto por categoría, evolución del patrimonio, distribución de cartera |
| Datos | Importar CSV con mapeo de columnas, exportar CSV y copia completa, papelera, historial de cambios |
| Experiencia | Moneda e idioma ajustables, tema claro/oscuro, modo privado que oculta importes, indicador de sincronización |

### Fuera del alcance

Conexión directa con bancos, ejecución de operaciones o pagos, recomendaciones de inversión y cálculo fiscal oficial.

## Mapa de fases

| Fase | Nombre | Depende de |
|---|---|---|
| 0 | Cimientos y validaciones | - |
| 1 | Núcleo y base de datos | 0 |
| 2 | Autenticación y sincronización offline | 1 |
| 3 | App base en las tres plataformas | 2 |
| 4 | Deudas y registros periódicos | 3 |
| 5 | Análisis, presupuestos y metas | 3 |
| 6 | Inversiones | 3 |
| 7 | Actualización automática de precios | 6 |
| 8 | Productividad y datos | 4, 5, 6 |
| 9 | Endurecimiento y publicación | Todas |

Las fases 4, 5 y 6 pueden avanzar en cualquier orden.

---

## Fase 0: Cimientos y validaciones

**Objetivo:** resolver los riesgos que podrían cambiar la arquitectura antes de construir sobre ellos.

**Procesos**
- Inicializar Git y el workspace de Cargo con la estructura de `CLAUDE.md`.
- Configurar CI con formato, clippy, tests y compilación para Windows, Android y WASM.
- Crear el proyecto de Supabase y entornos separados de desarrollo y producción.
- **Prueba de egui en Android:** formulario real con campos de texto y numéricos en un dispositivo físico. Evaluar teclado, tildes, autocompletado, foco, desplazamiento y rotación.
- **Prueba de fuentes:** medir cobertura global, límites y licencia de candidatos en sus páginas oficiales.
- **Prueba de WASM:** confirmar que `core`, `reqwest` e IndexedDB funcionan en navegador.

**Entregables:** repositorio con CI, APK de prueba, informe de fuentes de precios y decisión documentada para Android.

**Criterio de salida:** los tres targets compilan en CI y se decide, con evidencia, entre egui o PWA para Android y qué fuentes de precios usar.

## Fase 1: Núcleo y base de datos

**Objetivo:** tener el modelo de dominio y los cálculos correctos y probados, sin interfaz.

**Procesos**
- Modelar en `core`: usuario, cuenta, movimiento, categoría, etiqueta, contacto, deuda, cuota, regla periódica, presupuesto, meta, activo, operación, precio y tipo de cambio.
- Implementar dinero con moneda, conversiones, saldos y resúmenes.
- Escribir las migraciones SQL con RLS en todas las tablas de usuario.
- Probar los cálculos con casos límite y pruebas basadas en propiedades.
- Probar con dos usuarios que RLS impide acceder a datos ajenos.

**Entregables:** crate `core`, migraciones versionadas y suite de pruebas.

**Criterio de salida:** los cálculos pasan las pruebas sin usar flotantes y RLS bloquea el 100 % de los accesos cruzados probados.

## Fase 2: Autenticación y sincronización offline

**Objetivo:** que los datos se guarden siempre en local y lleguen solos al servidor.

**Procesos**
- Construir el cliente Supabase: registro, login, renovación de sesión, recuperación y consultas.
- Definir `LocalStore` e implementarlo en SQLite e IndexedDB.
- Implementar cola de salida, cursor de descarga, reloj lógico híbrido y último cambio por registro.
- Detectar conectividad y reintentar con espera progresiva.
- Probar particiones de red, relojes desajustados, reintentos, borrado frente a edición y sesión expirada estando offline.

**Entregables:** crates `supabase`, `sync`, `store-sqlite` y `store-idb`, más un simulador de dos dispositivos.

**Criterio de salida:** dos dispositivos simulados editan offline, se reconectan y convergen al mismo estado, sin duplicados ni pérdidas fuera de la regla del último cambio.

## Fase 3: App base en las tres plataformas

**Objetivo:** que una persona pueda usar la app a diario para registrar su dinero.

**Procesos**
- Crear `app-state` con casos de uso y datos listos para mostrar.
- Definir un sistema visual común: colores, tipografía, formato de moneda y fechas.
- Construir en egui y Leptos: registro/login, panel inicial, cuentas, movimientos, categorías, etiquetas, ajustes y papelera.
- Añadir búsqueda, filtros, listas paginadas e indicador de sincronización.
- Hacer la web instalable como PWA con service worker.
- Internacionalización inicial en español e inglés.

**Entregables:** ejecutable de Windows, APK y web funcionando contra Supabase.

**Criterio de salida:** registrar un gasto en menos de 10 segundos en las tres plataformas; un movimiento creado offline aparece en otro dispositivo tras reconectar.

## Fase 4: Deudas y registros periódicos

**Objetivo:** controlar lo que se debe, lo que te deben y lo que se repite.

**Procesos**
- Contactos con saldo neto e historial.
- Deudas en ambos sentidos con cuotas, interés opcional, pagos parciales y vencimientos.
- Opción de que un pago genere un movimiento en una cuenta.
- Gastos compartidos que crean deudas por persona.
- Motor de recurrencia: diaria, semanal, mensual, anual, personalizada, último día del mes.
- Modos automático y por confirmar, con vista de suscripciones.

**Entregables:** módulos de deudas y periódicos, y calendario básico de vencimientos.

**Criterio de salida:** un pago parcial actualiza deuda y cuenta correctamente; una regla procesada desde dos dispositivos offline no genera duplicados.

## Fase 5: Análisis, presupuestos y metas

**Objetivo:** convertir los registros en información clara.

**Procesos**
- Resúmenes mensuales y anuales con comparación entre periodos.
- Presupuestos por categoría, con opción de arrastrar el sobrante.
- Metas de ahorro con aportes y proyección de fecha.
- Series de datos de gráficos generadas en `app-state`; dibujo con `egui_plot` y SVG en Leptos.
- Panel inicial con patrimonio neto, flujo, presupuestos y vencimientos.
- Calendario financiero completo y simulador de pago de deudas (solo calcula escenarios del usuario).

**Entregables:** pantallas de informes, presupuestos, metas y gráficos.

**Criterio de salida:** los totales de cada gráfico coinciden con los movimientos filtrados, con datos de prueba de varios años y sin bloquear la interfaz.

## Fase 6: Inversiones

**Objetivo:** registrar la cartera completa y su rendimiento.

**Procesos**
- Catálogo global de activos, con búsqueda por nombre o ticker y alta manual.
- Operaciones de compra, venta, dividendo, comisión, split y transferencia entre brokers.
- Precio promedio, ganancia realizada y no realizada, por activo y total, con conversión de moneda.
- Vínculo opcional de cada operación con un movimiento de cuenta.
- Activos sin cotización con valor manual: inmuebles, depósitos, bonos.
- Distribución de cartera y evolución de valor.

**Entregables:** módulo de inversiones usando precios manuales.

**Criterio de salida:** una cartera de referencia en varias monedas, con splits y ventas parciales, produce los resultados calculados de antemano.

## Fase 7: Actualización automática de precios

**Objetivo:** que cada activo en cartera actualice su precio solo, 1 o 2 veces al día. Ejemplo: si un usuario tiene 2 acciones de una empresa del S&P 500, el valor de esa posición se recalcula con cada nuevo precio.

**Procesos**
- Implementar `PriceProvider` con las fuentes elegidas en la Fase 0, con respaldo por tipo de activo.
- Crear `price-worker`: reunir los símbolos únicos en poder de algún usuario, consultar por lotes respetando límites y hacer upsert por activo y fecha.
- Programarlo 1 o 2 veces al día con GitHub Actions (según permita la cuota de la fuente), con registro de ejecuciones y aviso de fallos.
- Actualizar tipos de cambio diarios.
- Guardar instantáneas de valor de cartera para el histórico.
- Alertas de precio evaluadas tras cada actualización.

**Entregables:** worker en producción e histórico de precios.

**Criterio de salida:** siete días seguidos de ejecuciones correctas; las posiciones se revalorizan solas con cada precio nuevo; si una fuente falla, la app conserva el último precio y muestra su fecha.

## Fase 8: Productividad y datos

**Objetivo:** reducir el trabajo manual y garantizar que los datos son del usuario.

**Procesos**
- Importación CSV con mapeo de columnas, vista previa y detección de duplicados.
- Exportación CSV y copia completa, con restauración.
- Reglas de autocategorización y plantillas rápidas.
- Adjuntos de comprobantes en Supabase Storage, por usuario.
- Historial de cambios, PIN local, modo privado.
- Avisos de vencimientos y alertas dentro de la app; notificaciones nativas de Android si la Fase 0 confirma que son viables.

**Entregables:** módulo de datos, adjuntos y avisos.

**Criterio de salida:** exportar, borrar y restaurar deja una cuenta de prueba idéntica a la original.

## Fase 9: Endurecimiento y publicación

**Objetivo:** dejarla lista para que la usen desconocidos con datos reales.

**Procesos**
- Auditoría de RLS, secretos y dependencias (`cargo audit`).
- Pruebas de rendimiento con 100 000 movimientos.
- Pruebas de extremo a extremo con un Supabase local (Docker), donde Claude crea cuentas de prueba y recorre registro, confirmación, login, recuperación de contraseña, sincronización entre dispositivos y eliminación de cuenta en las tres plataformas. Después, el usuario repite el registro en el Supabase real.
- Migración de versiones de la base local.
- Copias de seguridad del servidor y prueba de restauración.
- Confirmación de correo, límites contra abuso y eliminación total de cuenta.
- Política de privacidad, términos y aviso de que la app no asesora.
- Confirmar que las licencias de datos permiten el uso público; contratar plan de pago si hace falta.
- Instalador firmado de Windows, APK/AAB firmado, despliegue de la web y actualización de versiones.

**Entregables:** versión 1.0 publicada en las tres plataformas.

**Criterio de salida:** sin hallazgos críticos abiertos, restauración de copia probada y licencias de datos confirmadas.
