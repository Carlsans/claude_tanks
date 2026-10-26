package java.lang;

import java.util.IdentityHashMap;
import java.util.Map;
import java.util.function.Supplier;

/** jvm2rs shim: one value per thread, kept in a map keyed by thread. */
public class ThreadLocal<T> {
    private final Map<Thread, T> values = new IdentityHashMap<>();
    private final Map<Thread, Boolean> set = new IdentityHashMap<>();

    public ThreadLocal() {}

    protected T initialValue() { return null; }

    public static <S> ThreadLocal<S> withInitial(Supplier<? extends S> supplier) {
        return new ThreadLocal<S>() {
            protected S initialValue() { return supplier.get(); }
        };
    }

    public T get() {
        Thread t = Thread.currentThread();
        if (!set.containsKey(t)) {
            T v = initialValue();
            values.put(t, v);
            set.put(t, Boolean.TRUE);
            return v;
        }
        return values.get(t);
    }

    public void set(T value) {
        Thread t = Thread.currentThread();
        values.put(t, value);
        set.put(t, Boolean.TRUE);
    }

    public void remove() {
        Thread t = Thread.currentThread();
        values.remove(t);
        set.remove(t);
    }
}
