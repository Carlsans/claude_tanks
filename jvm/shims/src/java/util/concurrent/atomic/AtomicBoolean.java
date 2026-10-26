package java.util.concurrent.atomic;

/** jvm2rs shim: green threads never preempt, so a plain field is atomic. */
public class AtomicBoolean implements java.io.Serializable {
    private boolean value;

    public AtomicBoolean() {}
    public AtomicBoolean(boolean initialValue) { value = initialValue; }

    public final boolean get() { return value; }
    public final void set(boolean v) { value = v; }
    public final void lazySet(boolean v) { value = v; }
    public final boolean getPlain() { return value; }
    public final boolean getAcquire() { return value; }
    public final void setRelease(boolean v) { value = v; }

    public final boolean getAndSet(boolean v) {
        boolean old = value;
        value = v;
        return old;
    }

    public final boolean compareAndSet(boolean expect, boolean update) {
        if (value != expect) return false;
        value = update;
        return true;
    }

    public boolean weakCompareAndSet(boolean expect, boolean update) { return compareAndSet(expect, update); }

    public String toString() { return Boolean.toString(value); }
}
