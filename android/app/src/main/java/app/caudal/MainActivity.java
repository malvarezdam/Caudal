package app.caudal;

import android.Manifest;
import android.content.Context;
import android.content.Intent;
import android.content.SharedPreferences;
import android.content.pm.PackageManager;
import android.database.Cursor;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.provider.OpenableColumns;
import android.view.View;

import androidx.annotation.NonNull;
import androidx.core.graphics.Insets;
import androidx.core.view.WindowInsetsCompat;
import androidx.work.ExistingPeriodicWorkPolicy;
import androidx.work.ExistingWorkPolicy;
import androidx.work.OneTimeWorkRequest;
import androidx.work.PeriodicWorkRequest;
import androidx.work.WorkManager;

import com.google.androidgamesdk.GameActivity;

import java.io.ByteArrayOutputStream;
import java.io.InputStream;
import java.io.OutputStream;
import java.util.concurrent.TimeUnit;

public class MainActivity extends GameActivity {
    static {
        System.loadLibrary("caudal_native");
    }

    /** Códigos de las respuestas del selector de documentos. */
    private static final int OPEN_BASE = 1_000_000;
    private static final int SAVE = 2_000_000;
    private static final int REQ_NOTIFICATIONS = 3_000_000;
    /** Archivos mayores no se leen: la app acepta hasta 10 MB de comprobante. */
    private static final int MAX_BYTES = 32 * 1024 * 1024;
    private static final String DAILY_WORK_NAME = "caudal-daily-notices";
    private static final String ONCE_WORK_NAME = "caudal-notices-now";
    private static final String KEY_PERMISSION_ASKED = "permission_asked";

    /** Barras del sistema, muesca y teclado, en píxeles; la interfaz se dibuja dentro. */
    private static native void nativeSetInsets(int left, int top, int right, int bottom);

    /** Archivo elegido por el usuario; `data` es null si canceló o no se pudo leer. */
    private static native void nativeFileOpened(int request, String name, byte[] data);

    private static native void nativeFileSaved(boolean ok);

    private byte[] pendingSave;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        handleTriggerIntent(getIntent());
    }

    @Override
    protected void onNewIntent(Intent intent) {
        super.onNewIntent(intent);
        setIntent(intent);
        handleTriggerIntent(intent);
    }

    private void handleTriggerIntent(Intent intent) {
        if (intent != null && intent.getBooleanExtra("run_notices", false)) {
            enqueueImmediateNoticeCheck();
        }
    }

    private void scheduleDailyNotices() {
        PeriodicWorkRequest daily = new PeriodicWorkRequest.Builder(
                NoticeWorker.class, 24, TimeUnit.HOURS)
                .build();
        WorkManager.getInstance(this).enqueueUniquePeriodicWork(
                DAILY_WORK_NAME,
                ExistingPeriodicWorkPolicy.KEEP,
                daily);
    }

    private void enqueueImmediateNoticeCheck() {
        OneTimeWorkRequest request = new OneTimeWorkRequest.Builder(NoticeWorker.class).build();
        WorkManager.getInstance(this).enqueueUniqueWork(
                ONCE_WORK_NAME,
                ExistingWorkPolicy.REPLACE,
                request);
    }

    /**
     * Lo llama Rust al actualizar {@code files/notices.json}. El permiso solo se pide la
     * primera vez que existe algún aviso que mostrar, nunca al abrir la app sin avisos.
     */
    public void onNoticesUpdated(boolean hasNotices) {
        runOnUiThread(() -> {
            if (!hasNotices) {
                return;
            }
            if (Build.VERSION.SDK_INT >= 33
                    && checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS)
                            != PackageManager.PERMISSION_GRANTED) {
                SharedPreferences prefs = getSharedPreferences(
                        NoticeWorker.PREFS_NAME, Context.MODE_PRIVATE);
                if (!prefs.getBoolean(KEY_PERMISSION_ASKED, false)) {
                    prefs.edit().putBoolean(KEY_PERMISSION_ASKED, true).apply();
                    requestPermissions(
                            new String[]{Manifest.permission.POST_NOTIFICATIONS},
                            REQ_NOTIFICATIONS);
                }
                return;
            }
            scheduleDailyNotices();
        });
    }

    @Override
    public void onRequestPermissionsResult(
            int requestCode,
            @NonNull String[] permissions,
            @NonNull int[] grantResults) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults);
        if (requestCode == REQ_NOTIFICATIONS) {
            scheduleDailyNotices();
            if (grantResults.length > 0 && grantResults[0] == PackageManager.PERMISSION_GRANTED) {
                enqueueImmediateNoticeCheck();
            }
        }
    }

    @Override
    public WindowInsetsCompat onApplyWindowInsets(View view, WindowInsetsCompat insets) {
        WindowInsetsCompat result = super.onApplyWindowInsets(view, insets);
        Insets area = insets.getInsets(
                WindowInsetsCompat.Type.systemBars()
                        | WindowInsetsCompat.Type.displayCutout()
                        | WindowInsetsCompat.Type.ime());
        nativeSetInsets(area.left, area.top, area.right, area.bottom);
        return result;
    }

    /** Lo llama Rust: abre el selector para elegir un archivo. */
    public void openDocument(int request, String[] mimeTypes) {
        runOnUiThread(() -> {
            Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT);
            intent.addCategory(Intent.CATEGORY_OPENABLE);
            intent.setType("*/*");
            intent.putExtra(Intent.EXTRA_MIME_TYPES, mimeTypes);
            startActivityForResult(intent, OPEN_BASE + request);
        });
    }

    /** Lo llama Rust: pide dónde guardar y escribe los bytes allí. */
    public void createDocument(String name, String mimeType, byte[] data) {
        runOnUiThread(() -> {
            pendingSave = data;
            Intent intent = new Intent(Intent.ACTION_CREATE_DOCUMENT);
            intent.addCategory(Intent.CATEGORY_OPENABLE);
            intent.setType(mimeType);
            intent.putExtra(Intent.EXTRA_TITLE, name);
            startActivityForResult(intent, SAVE);
        });
    }

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent intent) {
        super.onActivityResult(requestCode, resultCode, intent);
        Uri uri = (resultCode == RESULT_OK && intent != null) ? intent.getData() : null;
        if (requestCode == SAVE) {
            byte[] data = pendingSave;
            pendingSave = null;
            boolean ok = false;
            if (uri != null && data != null) {
                try (OutputStream out = getContentResolver().openOutputStream(uri)) {
                    out.write(data);
                    ok = true;
                } catch (Exception ignored) {
                }
            }
            if (uri != null) {
                nativeFileSaved(ok);
            }
        } else if (requestCode >= OPEN_BASE) {
            int request = requestCode - OPEN_BASE;
            if (uri == null) {
                nativeFileOpened(request, null, null);
                return;
            }
            nativeFileOpened(request, displayName(uri), read(uri));
        }
    }

    private byte[] read(Uri uri) {
        try (InputStream in = getContentResolver().openInputStream(uri)) {
            ByteArrayOutputStream out = new ByteArrayOutputStream();
            byte[] buffer = new byte[64 * 1024];
            int count;
            while ((count = in.read(buffer)) != -1) {
                out.write(buffer, 0, count);
                if (out.size() > MAX_BYTES) {
                    return null;
                }
            }
            return out.toByteArray();
        } catch (Exception error) {
            return null;
        }
    }

    private String displayName(Uri uri) {
        try (Cursor cursor = getContentResolver().query(uri, null, null, null, null)) {
            if (cursor != null && cursor.moveToFirst()) {
                int index = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME);
                if (index >= 0) {
                    return cursor.getString(index);
                }
            }
        } catch (Exception ignored) {
        }
        return "archivo";
    }
}
