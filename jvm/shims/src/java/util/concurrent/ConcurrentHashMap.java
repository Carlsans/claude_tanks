package java.util.concurrent;

import java.util.Collections;
import java.util.HashMap;
import java.util.Map;
import java.util.Set;

/** jvm2rs shim: a HashMap that refuses nulls (threads never preempt). */
public class ConcurrentHashMap<K, V> extends HashMap<K, V> implements ConcurrentMap<K, V> {
    public ConcurrentHashMap() {}
    public ConcurrentHashMap(int initialCapacity) { super(initialCapacity); }
    public ConcurrentHashMap(int initialCapacity, float loadFactor) { super(initialCapacity, loadFactor); }
    public ConcurrentHashMap(int initialCapacity, float loadFactor, int concurrencyLevel) { super(initialCapacity, loadFactor); }
    public ConcurrentHashMap(Map<? extends K, ? extends V> m) { super(m); }

    public V put(K key, V value) {
        if (key == null || value == null) throw new NullPointerException();
        return super.put(key, value);
    }

    public V putIfAbsent(K key, V value) {
        if (key == null || value == null) throw new NullPointerException();
        return super.putIfAbsent(key, value);
    }

    public V get(Object key) {
        if (key == null) throw new NullPointerException();
        return super.get(key);
    }

    public static <K> KeySetView<K, Boolean> newKeySet() {
        return new KeySetView<K, Boolean>(new ConcurrentHashMap<K, Boolean>());
    }

    public KeySetView<K, V> keySet(V mappedValue) {
        return new KeySetView<K, V>(this);
    }

    /** A set backed by a map's keys. */
    public static class KeySetView<K, V> extends java.util.AbstractSet<K> {
        private final ConcurrentHashMap<K, V> map;
        private final Set<K> keys;

        KeySetView(ConcurrentHashMap<K, V> map) {
            this.map = map;
            this.keys = map.keySet();
        }

        @SuppressWarnings("unchecked")
        public boolean add(K k) { return map.put(k, (V) Boolean.TRUE) == null; }
        public boolean remove(Object o) { return map.remove(o) != null; }
        public boolean contains(Object o) { return map.containsKey(o); }
        public java.util.Iterator<K> iterator() { return keys.iterator(); }
        public int size() { return map.size(); }
        public void clear() { map.clear(); }
    }
}
