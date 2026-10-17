package com.mehrveil.app;

import android.app.Application;
import android.os.Process;
import android.widget.Toast;

import java.io.File;
import java.io.PrintWriter;

/** Crash safety: never silently die; write crash.txt for support. */
public class VeilApp extends Application {
    @Override
    public void onCreate() {
        super.onCreate();
        final Thread.UncaughtExceptionHandler prev = Thread.getDefaultUncaughtExceptionHandler();
        Thread.setDefaultUncaughtExceptionHandler((t, e) -> {
            try {
                PrintWriter pw = new PrintWriter(new File(getExternalFilesDir(null), "crash.txt"));
                e.printStackTrace(pw);
                pw.close();
            } catch (Throwable ignored) {
                try {
                    PrintWriter pw2 = new PrintWriter(new File(getFilesDir(), "crash.txt"));
                    e.printStackTrace(pw2);
                    pw2.close();
                } catch (Throwable ignored2) {}
            }
            try {
                Toast.makeText(this, "Mehrveil crashed — report Mehrveil/crash.txt", Toast.LENGTH_LONG).show();
            } catch (Throwable ignored) {}
            try { Thread.sleep(400); } catch (Throwable ignored) {}
            if (prev != null) prev.uncaughtException(t, e);
            else Process.killProcess(Process.myPid());
        });
    }
}
