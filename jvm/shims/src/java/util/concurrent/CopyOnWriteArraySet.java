package java.util.concurrent;

import java.util.AbstractSet;
import java.util.Collection;
import java.util.Iterator;

/** jvm2rs shim: a set over a copy-on-write list. */
public class CopyOnWriteArraySet<E> extends AbstractSet<E> implements java.io.Serializable {
    private final CopyOnWriteArrayList<E> list = new CopyOnWriteArrayList<E>();

    public CopyOnWriteArraySet() {}

    public CopyOnWriteArraySet(Collection<? extends E> c) { for (E e : c) add(e); }

    public boolean add(E e) { return list.addIfAbsent(e); }
    public boolean remove(Object o) { return list.remove(o); }
    public boolean contains(Object o) { return list.contains(o); }
    public Iterator<E> iterator() { return list.iterator(); }
    public int size() { return list.size(); }
    public void clear() { list.clear(); }
}
