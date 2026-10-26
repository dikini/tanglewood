package org.tanglewood;

import android.app.NativeActivity;
import android.content.Intent;
import android.net.Uri;
import android.os.CancellationSignal;
import android.os.ParcelFileDescriptor;

import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.util.concurrent.ArrayBlockingQueue;
import java.util.concurrent.ThreadPoolExecutor;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;

/** System SAF picker; only the native owner applies imported text to its App. */
public final class TanglewoodActivity extends NativeActivity {
    static { System.loadLibrary("tanglewood"); }
    private static final int PICK_DOCUMENT = 7331;
    private static final int OK = 0, CANCELLED = 1, FAILED = 2;
    private static native boolean nativeDocumentResult(long id, int status, String spool, String error);
    private static native byte[] nativeExportChunk(long id, long offset);
    private final Object lock = new Object();
    private final ThreadPoolExecutor io = new ThreadPoolExecutor(1, 1, 0, TimeUnit.MILLISECONDS,
        new ArrayBlockingQueue<>(1), runnable -> new Thread(runnable, "tanglewood-document-io"),
        new ThreadPoolExecutor.AbortPolicy());
    private Work pending;

    private static final class Work {
        final long id;
        final boolean exporting;
        volatile File spool;
        volatile boolean spoolTransferred;
        final AtomicBoolean completed = new AtomicBoolean();
        final AtomicBoolean cancelled = new AtomicBoolean();
        final CancellationSignal signal = new CancellationSignal();
        volatile ParcelFileDescriptor descriptor;
        Work(long id, boolean exporting) { this.id = id; this.exporting = exporting; }
    }

    /** Called from the native UI thread; all Activity APIs run on Android's main thread. */
    public void requestDocument(long id, boolean exporting, String title) {
        runOnUiThread(() -> {
            if (isFinishing() || isDestroyed()) {
                nativeDocumentResult(id, CANCELLED, "", "Activity is closing"); return;
            }
            Work work = new Work(id, exporting);
            synchronized (lock) {
                if (pending != null) {
                    nativeDocumentResult(id, FAILED, "", "Another document request is active"); return;
                }
                pending = work;
            }
            Intent intent = new Intent(exporting ? Intent.ACTION_CREATE_DOCUMENT : Intent.ACTION_OPEN_DOCUMENT);
            intent.addCategory(Intent.CATEGORY_OPENABLE);
            if (exporting) {
                intent.setType("text/markdown");
                intent.putExtra(Intent.EXTRA_TITLE, title);
                intent.addFlags(Intent.FLAG_GRANT_WRITE_URI_PERMISSION);
            } else {
                // Providers vary in Markdown MIME registration. Let the user select any document;
                // Rust validates UTF-8 and the shared command validates the destination path.
                intent.setType("*/*");
                intent.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION);
            }
            try { startActivityForResult(intent, PICK_DOCUMENT); }
            catch (RuntimeException error) { complete(work, FAILED, "", error.toString()); }
        });
    }

    @Override protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode, resultCode, data);
        if (requestCode != PICK_DOCUMENT) return;
        final Work work;
        synchronized (lock) { work = pending; }
        if (work == null) return;
        if (resultCode != RESULT_OK || data == null || data.getData() == null || work.cancelled.get()) {
            complete(work, CANCELLED, "", "Document picker cancelled"); return;
        }
        Uri uri = data.getData();
        try { io.execute(() -> transfer(work, uri)); }
        catch (RuntimeException error) { complete(work, FAILED, "", error.toString()); }
    }

    private void transfer(Work work, Uri uri) {
        try {
            if (work.cancelled.get()) { complete(work, CANCELLED, "", "Cancelled"); return; }
            // The URI is never converted to a filesystem path. Do not retain persistable grants.
            ParcelFileDescriptor descriptor = getContentResolver().openFileDescriptor(uri,
                work.exporting ? "wt" : "r", work.signal);
            if (descriptor == null) throw new IOException("Document provider returned no descriptor");
            work.descriptor = descriptor;
            if (work.cancelled.get()) { descriptor.close(); complete(work, CANCELLED, "", "Cancelled"); return; }
            String result = "";
            if (work.exporting) {
                try (OutputStream stream = new ParcelFileDescriptor.AutoCloseOutputStream(descriptor)) {
                    long offset = 0;
                    while (true) {
                        if (work.cancelled.get()) throw new IOException("Cancelled");
                        byte[] chunk = nativeExportChunk(work.id, offset);
                        if (chunk == null) throw new IOException("Export snapshot cancelled or unavailable");
                        if (chunk.length == 0) break;
                        if (chunk.length > 64 * 1024) throw new IOException("Invalid native export chunk");
                        stream.write(chunk);
                        if (offset > Long.MAX_VALUE - chunk.length) throw new IOException("Export offset exceeds platform range");
                        offset += chunk.length;
                    }
                    stream.flush();
                }
            } else {
                File directory = new File(getFilesDir(), "picker-transfer");
                if (!directory.isDirectory() && !directory.mkdirs()) throw new IOException("Cannot create private import directory");
                work.spool = File.createTempFile("import-", ".md", directory);
                try (InputStream stream = new ParcelFileDescriptor.AutoCloseInputStream(descriptor);
                     OutputStream spool = new FileOutputStream(work.spool)) {
                    byte[] chunk = new byte[64 * 1024];
                    int count;
                    while ((count = stream.read(chunk)) != -1) {
                        if (work.cancelled.get()) throw new IOException("Cancelled");
                        spool.write(chunk, 0, count);
                    }
                    spool.flush();
                }
                result = work.spool.getAbsolutePath();
            }
            complete(work, work.cancelled.get() ? CANCELLED : OK, result, "");
        } catch (Exception error) {
            complete(work, work.cancelled.get() ? CANCELLED : FAILED, "", error.toString());
        } finally {
            // Covers failures between obtaining the descriptor and constructing an AutoClose stream.
            ParcelFileDescriptor descriptor = work.descriptor;
            work.descriptor = null;
            if (descriptor != null) { try { descriptor.close(); } catch (IOException ignored) { } }
            if (work.spool != null && !work.spoolTransferred) work.spool.delete();
        }
    }

    private void complete(Work work, int status, String spool, String error) {
        if (!work.completed.compareAndSet(false, true)) return;
        synchronized (lock) { if (pending == work) pending = null; }
        String boundedError = error.length() > 2048 ? error.substring(0, 2048) : error;
        boolean accepted = nativeDocumentResult(work.id, status, spool, boundedError);
        work.spoolTransferred = status == OK && work.spool != null && accepted;
    }

    public void cancelDocument(long id) {
        runOnUiThread(() -> {
            Work work;
            synchronized (lock) { work = pending; }
            if (work == null || work.id != id) return;
            cancel(work);
            // Keep the pending request identity until onActivityResult returns, so a late result
            // cannot be mistaken for a new picker using the same Android request code.
            finishActivity(PICK_DOCUMENT);
        });
    }
    private void cancel(Work work) {
        work.cancelled.set(true);
        work.signal.cancel();
        ParcelFileDescriptor descriptor = work.descriptor;
        if (descriptor != null) { try { descriptor.close(); } catch (IOException ignored) { } }
    }
    @Override protected void onDestroy() {
        Work work;
        synchronized (lock) { work = pending; }
        if (work != null) {
            cancel(work);
            complete(work, CANCELLED, "", "Activity destroyed");
        }
        io.shutdownNow();
        super.onDestroy();
    }
}
