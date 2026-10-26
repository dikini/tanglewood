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
    private static final java.util.concurrent.atomic.AtomicLong NEXT_NATIVE_OWNER = new java.util.concurrent.atomic.AtomicLong();
    private final long nativeOwner = NEXT_NATIVE_OWNER.incrementAndGet();
    private static native void nativeBegin(long owner);
    private static native void nativeEnd(long owner);
    private static native void nativeSystemInsets(long owner, int left, int top, int right, int bottom, int width, int height);
    @Override protected void onCreate(android.os.Bundle state) {
        nativeBegin(nativeOwner);
        super.onCreate(state);
        getWindow().getDecorView().setOnApplyWindowInsetsListener((view, insets) -> {
            if (android.os.Build.VERSION.SDK_INT >= 30) {
                android.graphics.Insets bars = insets.getInsets(
                    android.view.WindowInsets.Type.systemBars() | android.view.WindowInsets.Type.displayCutout()
                    | android.view.WindowInsets.Type.ime());
                android.graphics.Rect bounds = getWindowManager().getCurrentWindowMetrics().getBounds();
                nativeSystemInsets(nativeOwner, bars.left, bars.top, bars.right, bars.bottom, bounds.width(), bounds.height());
            } else {
                android.util.DisplayMetrics metrics = new android.util.DisplayMetrics();
                getWindowManager().getDefaultDisplay().getRealMetrics(metrics);
                nativeSystemInsets(nativeOwner, insets.getSystemWindowInsetLeft(), insets.getSystemWindowInsetTop(),
                    insets.getSystemWindowInsetRight(), insets.getSystemWindowInsetBottom(), metrics.widthPixels, metrics.heightPixels);
            }
            return insets; // Preserve NativeActivity/adjustResize dispatch, including the IME.
        });
        getWindow().getDecorView().requestApplyInsets();
    }

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

    private interface InputOperation { boolean run(); }
    private MirrorEditText inputMirror;
    private long inputWidget, inputEpoch, inputSeq;
    private boolean changingInput;
    private int inputBatch;
    private static native boolean nativeInput(long owner, long widget, long epoch, long seq,
        String text, int start, int end, int composeStart, int composeEnd);

    /** Complete state updates coalesce in Rust; Editable, never a manual delete count, owns UTF-16. */
    private void reportInput() {
        if (changingInput || inputBatch != 0 || inputMirror == null || inputWidget == 0) return;
        android.text.Editable text = inputMirror.getText();
        nativeInput(nativeOwner, inputWidget, inputEpoch, ++inputSeq, text.toString(),
            inputMirror.getSelectionStart(), inputMirror.getSelectionEnd(),
            android.view.inputmethod.BaseInputConnection.getComposingSpanStart(text),
            android.view.inputmethod.BaseInputConnection.getComposingSpanEnd(text));
    }
    private final class MirrorEditText extends android.widget.EditText {
        MirrorEditText() {
            super(TanglewoodActivity.this);
            setSingleLine(false);
            setInputType(android.text.InputType.TYPE_CLASS_TEXT | android.text.InputType.TYPE_TEXT_FLAG_MULTI_LINE);
            setImeOptions(android.view.inputmethod.EditorInfo.IME_FLAG_NO_EXTRACT_UI);
            setAlpha(0f); setFocusableInTouchMode(true);
            addTextChangedListener(new android.text.TextWatcher() {
                public void beforeTextChanged(CharSequence s,int start,int count,int after) {}
                public void onTextChanged(CharSequence s,int start,int before,int count) { reportInput(); }
                public void afterTextChanged(android.text.Editable text) {}
            });
        }
        @Override protected void onSelectionChanged(int start,int end) {
            super.onSelectionChanged(start,end); reportInput();
        }
        @Override public android.view.inputmethod.InputConnection onCreateInputConnection(android.view.inputmethod.EditorInfo info) {
            android.view.inputmethod.InputConnection base = super.onCreateInputConnection(info);
            final long widget=inputWidget, epoch=inputEpoch;
            return new android.view.inputmethod.InputConnectionWrapper(base,false) {
                private boolean current() { return inputWidget==widget && inputEpoch==epoch && !isFinishing(); }
                private boolean change(InputOperation operation) {
                    if (!current()) return false;
                    inputBatch++;
                    try { return operation.run(); }
                    finally { inputBatch--; reportInput(); }
                }
                @Override public boolean beginBatchEdit() { if(!current())return false; inputBatch++; return super.beginBatchEdit(); }
                @Override public boolean endBatchEdit() { if(!current())return false; boolean result=super.endBatchEdit(); if(inputBatch>0)inputBatch--; reportInput(); return result; }
                @Override public boolean commitText(CharSequence text,int position) { return change(()->super.commitText(text,position)); }
                @Override public boolean setComposingText(CharSequence text,int position) { return change(()->super.setComposingText(text,position)); }
                @Override public boolean setComposingRegion(int start,int end) { return change(()->super.setComposingRegion(start,end)); }
                @Override public boolean finishComposingText() { return change(()->super.finishComposingText()); }
                @Override public boolean setSelection(int start,int end) { return change(()->super.setSelection(start,end)); }
                @Override public boolean deleteSurroundingText(int before,int after) { return change(()->super.deleteSurroundingText(before,after)); }
                @Override public boolean deleteSurroundingTextInCodePoints(int before,int after) { return change(()->super.deleteSurroundingTextInCodePoints(before,after)); }
                @Override public boolean sendKeyEvent(android.view.KeyEvent event) { return change(()->super.sendKeyEvent(event)); }
                @Override public boolean performContextMenuAction(int id) { return change(()->super.performContextMenuAction(id)); }
                @Override public boolean performEditorAction(int action) { return change(()->super.performEditorAction(action)); }
                @Override public boolean commitText(CharSequence text,int position,android.view.inputmethod.TextAttribute attributes) { return change(()->super.commitText(text,position,attributes)); }
                @Override public boolean setComposingText(CharSequence text,int position,android.view.inputmethod.TextAttribute attributes) { return change(()->super.setComposingText(text,position,attributes)); }
                @Override public boolean setComposingRegion(int start,int end,android.view.inputmethod.TextAttribute attributes) { return change(()->super.setComposingRegion(start,end,attributes)); }
            };
        }
    }
    private final Object inputQueueLock = new Object();
    private final java.util.ArrayDeque<InputJob> inputJobs = new java.util.ArrayDeque<>();
    private final android.os.Handler inputHandler = new android.os.Handler(android.os.Looper.getMainLooper());
    private boolean inputDrainPosted;
    private long inputBarrier;
    private static native void nativeInputFailure(long owner,String error);
    private static final class InputJob {
        final boolean snapshot, show;
        final long barrier;
        final Runnable operation;
        InputJob(boolean snapshot,boolean show,long barrier,Runnable operation) {
            this.snapshot=snapshot; this.show=show; this.barrier=barrier; this.operation=operation;
        }
    }
    /** One posted drain; complete snapshots coalesce only before the next captured action. */
    private void enqueueInput(long owner,boolean snapshot,boolean ending,boolean show,Runnable operation) {
        boolean full=false;
        synchronized(inputQueueLock) {
            if(owner!=nativeOwner) return;
            if(ending) { inputBarrier++; inputJobs.clear(); }
            if(snapshot && !inputJobs.isEmpty() && inputJobs.peekLast().snapshot) show |= inputJobs.removeLast().show;
            if(inputJobs.size()>=8) full=true;
            else {
                inputJobs.addLast(new InputJob(snapshot,show,inputBarrier,operation));
                if(!inputDrainPosted) { inputDrainPosted=true; inputHandler.post(this::drainInput); }
            }
        }
        if(full) nativeInputFailure(owner,"Android input queue is full; no text truncated. Rust draft retained.");
    }
    private void drainInput() {
        InputJob job;
        boolean current;
        synchronized(inputQueueLock) {
            job=inputJobs.pollFirst();
            if(job==null) {inputDrainPosted=false;return;}
            current=job.barrier==inputBarrier;
        }
        try {
            if(current && !isFinishing() && !isDestroyed()) {
                job.operation.run();
                if(job.show && inputMirror!=null) ((android.view.inputmethod.InputMethodManager)getSystemService(INPUT_METHOD_SERVICE))
                    .showSoftInput(inputMirror,android.view.inputmethod.InputMethodManager.SHOW_IMPLICIT);
            }
        }
        finally {
            synchronized(inputQueueLock) {
                if(inputJobs.isEmpty()) inputDrainPosted=false;
                else inputHandler.post(this::drainInput);
            }
        }
    }
    public void updateInput(long owner,long widget,long epoch,long ack,String text,int start,int end,int composeStart,int composeEnd,boolean show) {
        enqueueInput(owner,true,false,show,()->{
            if(owner!=nativeOwner || isFinishing() || isDestroyed()) return;
            if(inputMirror==null) {
                inputMirror=new MirrorEditText();
                addContentView(inputMirror,new android.view.ViewGroup.LayoutParams(1,1));
            }
            boolean restart=inputWidget!=widget || inputEpoch!=epoch;
            if(!restart && inputSeq>ack) return; // An old Rust echo cannot erase pending IME edits.
            changingInput=true;
            try {
                inputWidget=widget; inputEpoch=epoch;
                if(restart) { inputSeq=0; inputBatch=0; }
                if(!inputMirror.getText().toString().equals(text)) inputMirror.setText(text);
                inputMirror.setSelection(start,end);
                if(restart) android.view.inputmethod.BaseInputConnection.removeComposingSpans(inputMirror.getText());
                inputMirror.requestFocus();
            } finally { changingInput=false; }
            android.view.inputmethod.InputMethodManager imm=(android.view.inputmethod.InputMethodManager)getSystemService(INPUT_METHOD_SERVICE);
            if(restart) imm.restartInput(inputMirror);

        });
    }
    public void endInput(long owner) {
        enqueueInput(owner,false,true,false,()->{
            if(owner!=nativeOwner || inputMirror==null) return;
            inputWidget=0; inputEpoch++; inputBatch=0;
            ((android.view.inputmethod.InputMethodManager)getSystemService(INPUT_METHOD_SERVICE))
                .hideSoftInputFromWindow(inputMirror.getWindowToken(),0);
        });
    }
    public void inputAction(long owner,long widget,long epoch,int action) {
        enqueueInput(owner,false,false,false,()->{
            if(owner!=nativeOwner || inputMirror==null || inputWidget!=widget || inputEpoch!=epoch) return;
            inputBatch++;
            try {
                switch(action) {
                    case 1: inputMirror.selectAll(); break;
                    case 2: inputMirror.onTextContextMenuItem(android.R.id.copy); break;
                    case 3: inputMirror.onTextContextMenuItem(android.R.id.cut); break;
                    case 4: inputMirror.onTextContextMenuItem(android.R.id.paste); break;
                    default: return;
                }
            } finally { inputBatch--; reportInput(); }
        });
    }
    public void copyInputText(long owner,String text) {
        enqueueInput(owner,false,false,false,()->{
            if(owner==nativeOwner) ((android.content.ClipboardManager)getSystemService(CLIPBOARD_SERVICE))
                .setPrimaryClip(android.content.ClipData.newPlainText("Tanglewood",text));
        });
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
        synchronized(inputQueueLock) {inputBarrier++;inputJobs.clear();}
        nativeEnd(nativeOwner);
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
