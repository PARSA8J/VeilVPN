package com.veilvpn.app;

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
            } catch (Throwable ignored) {}
            try {
                Toast.makeText(this, "VeilVPN 1.5.1 crash — restart", Toast.LENGTH_LONG).show();
            } catch (Throwable ignored) {}
            try { Thread.sleep(400); } catch (Throwable ignored) {}
            if (prev != null) prev.uncaughtException(t, e);
            else Process.killProcess(Process.myPid());
        });
    }
}
