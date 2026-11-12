package app.caudal;

import android.Manifest;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.content.Context;
import android.content.Intent;
import android.content.SharedPreferences;
import android.content.pm.PackageManager;
import android.os.Build;

import androidx.annotation.NonNull;
import androidx.core.app.NotificationCompat;
import androidx.work.Worker;
import androidx.work.WorkerParameters;

import org.json.JSONArray;
import org.json.JSONObject;

import java.io.File;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.time.LocalDate;
import java.util.HashSet;
import java.util.Set;

/**
 * Publica una vez al día los avisos que la app dejó preparados en {@code files/notices.json}.
 * No ejecuta Rust en segundo plano ni usa la red.
 */
public class NoticeWorker extends Worker {
    static final String PREFS_NAME = "caudal_notices";
    private static final String CHANNEL_ID = "caudal_notices";
    private static final String KEY_SENT_DATE = "sent_date";
    private static final String KEY_SENT_ITEMS = "sent_items";

    public NoticeWorker(@NonNull Context context, @NonNull WorkerParameters params) {
        super(context, params);
    }

    @NonNull
    @Override
    public Result doWork() {
        Context context = getApplicationContext();
        if (Build.VERSION.SDK_INT >= 33
                && context.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS)
                        != PackageManager.PERMISSION_GRANTED) {
            return Result.success();
        }
        NotificationManager manager = context.getSystemService(NotificationManager.class);
        if (manager == null || !manager.areNotificationsEnabled()) {
            return Result.success();
        }

        File file = new File(context.getFilesDir(), "notices.json");
        if (!file.isFile()) {
            return Result.success();
        }

        try {
            String raw = new String(Files.readAllBytes(file.toPath()), StandardCharsets.UTF_8);
            JSONObject root = new JSONObject(raw);
            String title = root.optString("title", "Caudal");
            JSONArray items = root.optJSONArray("items");
            if (items == null || items.length() == 0) {
                return Result.success();
            }

            String today = LocalDate.now().toString();
            SharedPreferences prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE);
            Set<String> sent = new HashSet<>();
            if (today.equals(prefs.getString(KEY_SENT_DATE, ""))) {
                Set<String> stored = prefs.getStringSet(KEY_SENT_ITEMS, null);
                if (stored != null) {
                    sent.addAll(stored);
                }
            }

            NotificationChannel channel = new NotificationChannel(
                    CHANNEL_ID,
                    title,
                    NotificationManager.IMPORTANCE_DEFAULT);
            manager.createNotificationChannel(channel);

            Intent openIntent = new Intent(context, MainActivity.class);
            openIntent.setFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP | Intent.FLAG_ACTIVITY_CLEAR_TOP);
            PendingIntent pendingIntent = PendingIntent.getActivity(
                    context,
                    0,
                    openIntent,
                    PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);

            boolean changed = false;
            for (int i = 0; i < items.length(); i++) {
                JSONObject item = items.optJSONObject(i);
                if (item == null) {
                    continue;
                }
                String date = item.optString("date", "");
                String text = item.optString("text", "");
                if (!today.equals(date) || text.isEmpty() || sent.contains(text)) {
                    continue;
                }
                int notificationId = 1000 + ((text.hashCode() & 0x7fffffff) % 100000);
                NotificationCompat.Builder builder = new NotificationCompat.Builder(context, CHANNEL_ID)
                        .setSmallIcon(android.R.drawable.ic_dialog_info)
                        .setContentTitle(title)
                        .setContentText(text)
                        .setStyle(new NotificationCompat.BigTextStyle().bigText(text))
                        .setPriority(NotificationCompat.PRIORITY_DEFAULT)
                        .setContentIntent(pendingIntent)
                        .setAutoCancel(true);
                manager.notify(notificationId, builder.build());
                sent.add(text);
                changed = true;
            }

            if (changed) {
                prefs.edit()
                        .putString(KEY_SENT_DATE, today)
                        .putStringSet(KEY_SENT_ITEMS, sent)
                        .apply();
            }
        } catch (Exception ignored) {
        }
        return Result.success();
    }
}
