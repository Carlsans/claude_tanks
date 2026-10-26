package java.util;

/**
 * jvm2rs shim: the classes behind List.of, Set.of and Map.of, with the
 * members those methods (translated from the JDK) use. Unmodifiable lists,
 * sets and maps that refuse nulls; sets and maps keep insertion order (the
 * JDK's order is randomized per run anyway).
 */
@SuppressWarnings({"unchecked", "rawtypes"})
class ImmutableCollections {
    static final ListN EMPTY_LIST = new ListN(new Object[0]);
    static final SetN EMPTY_SET = new SetN(new Object[0]);
    static final MapN EMPTY_MAP = new MapN(new Object[0]);

    static UnsupportedOperationException uoe() { return new UnsupportedOperationException(); }

    static <E> List<E> listFromArray(E... input) {
        Object[] a = new Object[input.length];
        for (int i = 0; i < input.length; i++) a[i] = Objects.requireNonNull(input[i]);
        return new ListN(a);
    }

    static <E> List<E> listFromTrustedArray(Object... input) { return listFromArray((E[]) input); }

    static <E> List<E> listFromTrustedArrayNullsAllowed(Object... input) { return new ListN(input.clone(), true); }

    static <E> List<E> listCopy(Collection<? extends E> coll) {
        if (coll instanceof ListN) return (List<E>) coll;
        return listFromArray((E[]) coll.toArray());
    }

    abstract static class AbstractImmutableCollection<E> extends AbstractCollection<E> {
        public boolean add(E e) { throw uoe(); }
        public boolean addAll(Collection<? extends E> c) { throw uoe(); }
        public void clear() { throw uoe(); }
        public boolean remove(Object o) { throw uoe(); }
        public boolean removeAll(Collection<?> c) { throw uoe(); }
        public boolean retainAll(Collection<?> c) { throw uoe(); }
    }

    abstract static class AbstractImmutableList<E> extends AbstractList<E> implements RandomAccess {
        public boolean add(E e) { throw uoe(); }
        public void add(int i, E e) { throw uoe(); }
        public boolean addAll(Collection<? extends E> c) { throw uoe(); }
        public void clear() { throw uoe(); }
        public E remove(int i) { throw uoe(); }
        public boolean remove(Object o) { throw uoe(); }
        public E set(int i, E e) { throw uoe(); }
        public void sort(Comparator<? super E> c) { throw uoe(); }
    }

    static final class List12<E> extends AbstractImmutableList<E> {
        private final Object e0, e1;
        List12(E e0) { this.e0 = Objects.requireNonNull(e0); this.e1 = null; }
        List12(E e0, E e1) { this.e0 = Objects.requireNonNull(e0); this.e1 = Objects.requireNonNull(e1); }
        public int size() { return e1 == null ? 1 : 2; }
        public E get(int i) {
            if (i == 0) return (E) e0;
            if (i == 1 && e1 != null) return (E) e1;
            throw new IndexOutOfBoundsException("Index: " + i + " Size: " + size());
        }
    }

    static final class ListN<E> extends AbstractImmutableList<E> {
        private final Object[] a;
        ListN(Object[] a) { this.a = a; }
        ListN(Object[] a, boolean nullsAllowed) { this.a = a; }
        public int size() { return a.length; }
        public E get(int i) { return (E) a[i]; }
        public Object[] toArray() { return a.clone(); }
    }

    abstract static class AbstractImmutableSet<E> extends AbstractSet<E> {
        public boolean add(E e) { throw uoe(); }
        public boolean addAll(Collection<? extends E> c) { throw uoe(); }
        public void clear() { throw uoe(); }
        public boolean remove(Object o) { throw uoe(); }
        public boolean removeAll(Collection<?> c) { throw uoe(); }
        public boolean retainAll(Collection<?> c) { throw uoe(); }
    }

    static final class Set12<E> extends AbstractImmutableSet<E> {
        private final List<E> items = new ArrayList<>(2);
        Set12(E e0) { items.add(Objects.requireNonNull(e0)); }
        Set12(E e0, E e1) {
            if (e0.equals(Objects.requireNonNull(e1))) throw new IllegalArgumentException("duplicate element: " + e0);
            items.add(e0);
            items.add(e1);
        }
        public int size() { return items.size(); }
        public boolean contains(Object o) { return items.contains(o); }
        public Iterator<E> iterator() { return Collections.unmodifiableList(items).iterator(); }
    }

    static final class SetN<E> extends AbstractImmutableSet<E> {
        private final LinkedHashSet<E> items = new LinkedHashSet<>();
        SetN(E... input) {
            for (E e : input) {
                if (!items.add(Objects.requireNonNull(e))) throw new IllegalArgumentException("duplicate element: " + e);
            }
        }
        public int size() { return items.size(); }
        public boolean contains(Object o) { return items.contains(o); }
        public Iterator<E> iterator() { return Collections.unmodifiableSet(items).iterator(); }
    }

    abstract static class AbstractImmutableMap<K, V> extends AbstractMap<K, V> {
        public void clear() { throw uoe(); }
        public V put(K key, V value) { throw uoe(); }
        public void putAll(Map<? extends K, ? extends V> m) { throw uoe(); }
        public V remove(Object key) { throw uoe(); }
        public V putIfAbsent(K key, V value) { throw uoe(); }
    }

    static final class Map1<K, V> extends AbstractImmutableMap<K, V> {
        private final K k0;
        private final V v0;
        Map1(K k0, V v0) { this.k0 = Objects.requireNonNull(k0); this.v0 = Objects.requireNonNull(v0); }
        public Set<Map.Entry<K, V>> entrySet() { return Set.of(new KeyValueHolder<>(k0, v0)); }
        public V get(Object o) { return o.equals(k0) ? v0 : null; }
        public boolean containsKey(Object o) { return o.equals(k0); }
        public int size() { return 1; }
    }

    static final class MapN<K, V> extends AbstractImmutableMap<K, V> {
        private final LinkedHashMap<K, V> map = new LinkedHashMap<>();
        MapN(Object... input) {
            if ((input.length & 1) != 0) throw new InternalError("length is odd");
            for (int i = 0; i < input.length; i += 2) {
                K k = (K) Objects.requireNonNull(input[i]);
                V v = (V) Objects.requireNonNull(input[i + 1]);
                if (map.put(k, v) != null) throw new IllegalArgumentException("duplicate key: " + k);
            }
        }
        public Set<Map.Entry<K, V>> entrySet() { return Collections.unmodifiableMap(map).entrySet(); }
        public V get(Object o) { return map.get(o); }
        public boolean containsKey(Object o) { return map.containsKey(o); }
        public int size() { return map.size(); }
    }
}
