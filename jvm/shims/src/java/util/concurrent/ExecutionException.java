package java.util.concurrent;

/** jvm2rs shim. */
public class ExecutionException extends Exception {
    public ExecutionException(String message) { super(message); }
    public ExecutionException(Throwable cause) { super(cause); }
    public ExecutionException(String message, Throwable cause) { super(message, cause); }
}
