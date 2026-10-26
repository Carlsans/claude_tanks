package java.util.concurrent;

/** jvm2rs shim: only the type; nothing composes stages here. */
public interface CompletionStage<T> {
    CompletableFuture<T> toCompletableFuture();
}
