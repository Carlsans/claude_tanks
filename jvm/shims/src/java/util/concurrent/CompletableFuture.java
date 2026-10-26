package java.util.concurrent;

/** jvm2rs shim: a value that is set once; get and join wait for it. */
public class CompletableFuture<T> implements Future<T>, CompletionStage<T> {
    private T value;
    private Throwable failure;
    private boolean done;

    public CompletableFuture() {}

    public static <U> CompletableFuture<U> completedFuture(U value) {
        CompletableFuture<U> f = new CompletableFuture<U>();
        f.complete(value);
        return f;
    }

    public synchronized boolean complete(T v) {
        if (done) return false;
        value = v;
        done = true;
        notifyAll();
        return true;
    }

    public synchronized boolean completeExceptionally(Throwable t) {
        if (done) return false;
        failure = t;
        done = true;
        notifyAll();
        return true;
    }

    public synchronized T join() {
        while (!done) {
            try { wait(); } catch (InterruptedException e) { throw new RuntimeException(e); }
        }
        if (failure != null) throw new RuntimeException(failure);
        return value;
    }

    public synchronized T get() throws InterruptedException, ExecutionException {
        while (!done) wait();
        if (failure != null) throw new ExecutionException(failure);
        return value;
    }

    public synchronized T getNow(T valueIfAbsent) { return done && failure == null ? value : valueIfAbsent; }
    public boolean cancel(boolean mayInterruptIfRunning) { return false; }
    public boolean isCancelled() { return false; }
    public synchronized boolean isDone() { return done; }
    public CompletableFuture<T> toCompletableFuture() { return this; }
}
