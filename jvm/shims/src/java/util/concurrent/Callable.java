package java.util.concurrent;

/** jvm2rs shim. */
@FunctionalInterface
public interface Callable<V> {
    V call() throws Exception;
}
