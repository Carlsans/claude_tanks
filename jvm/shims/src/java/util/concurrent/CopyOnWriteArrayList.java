package java.util.concurrent;

import java.util.AbstractList;
import java.util.Arrays;
import java.util.Collection;
import java.util.Iterator;
import java.util.List;
import java.util.NoSuchElementException;
import java.util.RandomAccess;

/** jvm2rs shim: every change copies the array; iterators see a snapshot. */
public class CopyOnWriteArrayList<E> extends AbstractList<E> implements List<E>, RandomAccess, Cloneable, java.io.Serializable {
    private Object[] array = new Object[0];

    public CopyOnWriteArrayList() {}

    public CopyOnWriteArrayList(Collection<? extends E> c) { array = c.toArray(); }

    @SuppressWarnings("unchecked")
    public E get(int index) { return (E) array[index]; }

    public int size() { return array.length; }

    @SuppressWarnings("unchecked")
    public E set(int index, E element) {
        Object[] a = Arrays.copyOf(array, array.length);
        E old = (E) a[index];
        a[index] = element;
        array = a;
        return old;
    }

    public boolean add(E e) {
        Object[] a = Arrays.copyOf(array, array.length + 1);
        a[array.length] = e;
        array = a;
        return true;
    }

    public void add(int index, E e) {
        if (index < 0 || index > array.length) throw new IndexOutOfBoundsException("Index: " + index);
        Object[] a = new Object[array.length + 1];
        System.arraycopy(array, 0, a, 0, index);
        a[index] = e;
        System.arraycopy(array, index, a, index + 1, array.length - index);
        array = a;
    }

    @SuppressWarnings("unchecked")
    public E remove(int index) {
        E old = (E) array[index];
        Object[] a = new Object[array.length - 1];
        System.arraycopy(array, 0, a, 0, index);
        System.arraycopy(array, index + 1, a, index, array.length - index - 1);
        array = a;
        return old;
    }

    public boolean remove(Object o) {
        int i = indexOf(o);
        if (i < 0) return false;
        remove(i);
        return true;
    }

    public boolean addIfAbsent(E e) {
        if (contains(e)) return false;
        return add(e);
    }

    public void clear() { array = new Object[0]; }

    public Iterator<E> iterator() {
        final Object[] snapshot = array;
        return new Iterator<E>() {
            int i = 0;
            public boolean hasNext() { return i < snapshot.length; }
            @SuppressWarnings("unchecked")
            public E next() {
                if (i >= snapshot.length) throw new NoSuchElementException();
                return (E) snapshot[i++];
            }
        };
    }

    public Object[] toArray() { return Arrays.copyOf(array, array.length); }
}
