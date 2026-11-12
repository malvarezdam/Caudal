package app.caudal;

import android.content.Intent;
import android.database.Cursor;
import android.net.Uri;
import android.provider.OpenableColumns;
import android.view.View;

import androidx.core.graphics.Insets;
import androidx.core.view.WindowInsetsCompat;

import com.google.androidgamesdk.GameActivity;

import java.io.ByteArrayOutputStream;
import java.io.InputStream;
import java.io.OutputStream;

public class MainActivity extends GameActivity {
    static {
        System.loadLibrary("caudal_native");
    }

    /** Códigos de las respuestas del selector de documentos. */
    private static final int OPEN_BASE = 1_000_000;
    private static final int SAVE = 2_000_000;
    /** Archivos mayores no se leen: la app acepta hasta 10 MB de comprobante. */
    private static final int MAX_BYTES = 32 * 1024 * 1024;

    /** Barras del sistema, muesca y teclado, en píxeles; la interfaz se dibuja dentro. */
    private static native void nativeSetInsets(int left, int top, int right, int bottom);

    /** Archivo elegido por el usuario; `data` es null si canceló o no se pudo leer. */
    private static native void nativeFileOpened(int request, String name, byte[] data);

    private static native void nativeFileSaved(boolean ok);

    private byte[] pendingSave;

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
