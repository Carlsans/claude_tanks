package java.util.concurrent.atomic;

import java.util.function.IntBinaryOperator;
import java.util.function.IntUnaryOperator;

/** jvm2rs shim: green threads never preempt, so a plain field is atomic. */
public class AtomicInteger extends Number implements java.io.Serializable {
    private int value;

    public AtomicInteger() {}
    public AtomicInteger(int initialValue) { value = initialValue; }

    public final int get() { return value; }
    public final void set(int v) { value = v; }
    public final void lazySet(int v) { value = v; }
    public final int getAndSet(int v) { int o = value; value = v; return o; }

    public final boolean compareAndSet(int expect, int update) {
        if (value != expect) return false;
        value = update;
        return true;
    }

    public final boolean weakCompareAndSet(int expect, int update) { return compareAndSet(expect, update); }
    public final int getAndIncrement() { return value++; }
    public final int getAndDecrement() { return value--; }
    public final int getAndAdd(int d) { int o = value; value += d; return o; }
    public final int incrementAndGet() { return ++value; }
    public final int decrementAndGet() { return --value; }
    public final int addAndGet(int d) { return value += d; }
    public final int getAndUpdate(IntUnaryOperator f) { int o = value; value = f.applyAsInt(o); return o; }
    public final int updateAndGet(IntUnaryOperator f) { return value = f.applyAsInt(value); }
    public final int getAndAccumulate(int x, IntBinaryOperator f) { int o = value; value = f.applyAsInt(o, x); return o; }
    public final int accumulateAndGet(int x, IntBinaryOperator f) { return value = f.applyAsInt(value, x); }

    public String toString() { return Integer.toString(value); }
    public int intValue() { return value; }
    public long longValue() { return value; }
    public float floatValue() { return value; }
    public double doubleValue() { return value; }
}
