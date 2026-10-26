package java.util.concurrent.atomic;

import java.util.function.BinaryOperator;
import java.util.function.UnaryOperator;

/** jvm2rs shim: green threads never preempt, so a plain field is atomic. */
public class AtomicReference<V> implements java.io.Serializable {
    private V value;

    public AtomicReference() {}
    public AtomicReference(V initialValue) { value = initialValue; }

    public final V get() { return value; }
    public final void set(V v) { value = v; }
    public final void lazySet(V v) { value = v; }
    public final V getAndSet(V v) { V o = value; value = v; return o; }

    public final boolean compareAndSet(V expect, V update) {
        if (value != expect) return false;
        value = update;
        return true;
    }

    public final boolean weakCompareAndSet(V expect, V update) { return compareAndSet(expect, update); }
    public final V getAndUpdate(UnaryOperator<V> f) { V o = value; value = f.apply(o); return o; }
    public final V updateAndGet(UnaryOperator<V> f) { return value = f.apply(value); }
    public final V getAndAccumulate(V x, BinaryOperator<V> f) { V o = value; value = f.apply(o, x); return o; }
    public final V accumulateAndGet(V x, BinaryOperator<V> f) { return value = f.apply(value, x); }

    public String toString() { return String.valueOf(value); }
}
