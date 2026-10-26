package java.util.concurrent.atomic;

import java.util.function.LongBinaryOperator;
import java.util.function.LongUnaryOperator;

/** jvm2rs shim: green threads never preempt, so a plain field is atomic. */
public class AtomicLong extends Number implements java.io.Serializable {
    private long value;

    public AtomicLong() {}
    public AtomicLong(long initialValue) { value = initialValue; }

    public final long get() { return value; }
    public final void set(long v) { value = v; }
    public final void lazySet(long v) { value = v; }
    public final long getAndSet(long v) { long o = value; value = v; return o; }

    public final boolean compareAndSet(long expect, long update) {
        if (value != expect) return false;
        value = update;
        return true;
    }

    public final boolean weakCompareAndSet(long expect, long update) { return compareAndSet(expect, update); }
    public final long getAndIncrement() { return value++; }
    public final long getAndDecrement() { return value--; }
    public final long getAndAdd(long d) { long o = value; value += d; return o; }
    public final long incrementAndGet() { return ++value; }
    public final long decrementAndGet() { return --value; }
    public final long addAndGet(long d) { return value += d; }
    public final long getAndUpdate(LongUnaryOperator f) { long o = value; value = f.applyAsLong(o); return o; }
    public final long updateAndGet(LongUnaryOperator f) { return value = f.applyAsLong(value); }
    public final long getAndAccumulate(long x, LongBinaryOperator f) { long o = value; value = f.applyAsLong(o, x); return o; }
    public final long accumulateAndGet(long x, LongBinaryOperator f) { return value = f.applyAsLong(value, x); }

    public String toString() { return Long.toString(value); }
    public int intValue() { return (int) value; }
    public long longValue() { return value; }
    public float floatValue() { return value; }
    public double doubleValue() { return value; }
}
