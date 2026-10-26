// Differential test: language features that compile to less common bytecode.
import java.io.Serializable;

interface Greeter { String PREFIX = "hi "; String greet(String who); }

interface Loud extends Greeter {
    default String shout(String who) { return greet(who).toUpperCase() + "!"; }
}

class Polite implements Loud {
    public String greet(String who) { return PREFIX + who; }
}

class Node implements Cloneable, Comparable<Node> {
    int value;
    Node next;
    Node(int v, Node n) { value = v; next = n; }
    public int compareTo(Node o) { return Integer.compare(value, o.value); }
    public Node clone() {
        try { return (Node) super.clone(); } catch (CloneNotSupportedException e) { throw new RuntimeException(e); }
    }
}

class Outer {
    private int secret = 5;
    class Inner { int peek() { return secret * 2; } }
    static class Nested { static int twice(int x) { return x + x; } }
    int viaInner() { return new Inner().peek(); }
}

class Fails {
    static int value;
    static { if (true) value = 1 / zero(); }
    static int zero() { return 0; }
}

enum Dir {
    NORTH(0, 1), EAST(1, 0), SOUTH(0, -1), WEST(-1, 0);
    final int dx, dy;
    Dir(int dx, int dy) { this.dx = dx; this.dy = dy; }
    Dir turn() { return values()[(ordinal() + 1) % 4]; }
}

public class Main {
    static long longs(long a, long b) {
        long[] arr = { a, b, a * b };
        arr[1] += arr[2]--;
        double d = arr[0];
        d *= 1.5;
        return arr[1] + (long) d + arr[2];
    }

    static int labeled() {
        int count = 0;
        outer:
        for (int i = 0; i < 10; i++) {
            for (int j = 0; j < 10; j++) {
                if (j == i) continue outer;
                if (i * j > 20) break outer;
                count += i ^ j;
            }
        }
        return count;
    }

    static String stringSwitch(String s) {
        switch (s) {
            case "alpha": return "A";
            case "beta": return "B";
            case "Aa": return "collide1";
            case "BB": return "collide2";
            default: return "?";
        }
    }

    static int tryReturn() {
        int x = 1;
        try {
            x = 2;
            return x;
        } finally {
            x = 3;
            System.out.println("finally sees " + x);
        }
    }

    static int nested(int k) {
        try {
            try {
                if (k > 0) throw new IllegalStateException("inner " + k);
                return 0;
            } catch (IllegalArgumentException e) {
                return -1;
            } finally {
                System.out.println("inner finally " + k);
            }
        } catch (RuntimeException e) {
            System.out.println("outer caught " + e.getMessage());
            return k * 10;
        }
    }

    static final Object LOCK = new Object();
    static int counter;

    static synchronized void bump() { counter++; }

    public static void main(String[] args) {
        Polite p = new Polite();
        Greeter g = p;
        Loud l = p;
        System.out.println(g.greet("bob") + " / " + l.shout("ann") + " / " + Greeter.PREFIX.length());
        Node a = new Node(3, null), b = new Node(1, a);
        Node c = b.clone();
        System.out.println("clone " + (c != b) + " " + (c.next == a) + " " + c.value + " " + b.compareTo(a) + " " + a.compareTo(b));
        Comparable<Node> cmp = a;
        System.out.println("comparable " + cmp.compareTo(b));
        int[] xs = { 4, 1, 3 };
        int[] ys = xs.clone();
        ys[0] = 99;
        System.out.println("array clone " + xs[0] + " " + ys[0] + " " + ys.length);
        Object arr = ys;
        System.out.println("array types " + (arr instanceof int[]) + " " + (arr instanceof Object[]) + " " + (arr instanceof Cloneable) + " " + (arr instanceof Serializable));
        String[] names = { "x", "y" };
        Object[] objs = names;
        try {
            objs[0] = Integer.valueOf(1);
        } catch (ArrayStoreException e) {
            System.out.println("array store check");
        }
        System.out.println("outer " + new Outer().viaInner() + " " + Outer.Nested.twice(21));
        try {
            System.out.println(Fails.value);
        } catch (ExceptionInInitializerError e) {
            System.out.println("init failed");
        } catch (ArithmeticException e) {
            System.out.println("init failed");
        }
        Dir d = Dir.NORTH;
        StringBuilder path = new StringBuilder();
        int x = 0, y = 0;
        for (int i = 0; i < 6; i++) { x += d.dx; y += d.dy; path.append(d.name().charAt(0)); d = d.turn(); }
        System.out.println("enum " + path + " " + x + "," + y + " " + Dir.valueOf("WEST").ordinal() + " " + Dir.values().length);
        System.out.println("longs " + longs(7, 9) + " " + longs(-3, Long.MAX_VALUE));
        System.out.println("labeled " + labeled());
        System.out.println("switch " + stringSwitch("alpha") + stringSwitch("beta") + stringSwitch("Aa") + stringSwitch("BB") + stringSwitch("zz"));
        System.out.println("tryReturn " + tryReturn());
        System.out.println("nested " + nested(0) + " " + nested(4));
        synchronized (LOCK) { bump(); bump(); }
        System.out.println("counter " + counter);
        char ch = 'a';
        ch += 2;
        ch++;
        int code = ch + 1;
        System.out.println("chars " + ch + " " + code + " " + (char) (ch - 32) + " " + Character.isLetter(ch) + " " + Character.toUpperCase(ch));
        short s = (short) 40000;
        byte by = (byte) (s >> 4);
        System.out.println("narrow " + s + " " + by + " " + (by & 0xff) + " " + (s >>> 20));
        float f = 1.1f;
        double sum = 0;
        for (int i = 0; i < 10; i++) sum += f;
        System.out.println("floats " + sum + " " + (f * 3) + " " + (float) sum + " " + (0.1 + 0.2) + " " + (float) (0.1 + 0.2) + " " + 1.0f / 3);
        long big = Long.MAX_VALUE;
        System.out.println("long edges " + (big + 1) + " " + (Long.MIN_VALUE / -1) + " " + (Integer.MIN_VALUE / -1) + " " + Math.abs(Integer.MIN_VALUE) + " " + (int) (big >> 32));
        double nan = Double.NaN;
        System.out.println("nan " + (nan == nan) + " " + (nan < 1) + " " + (nan > 1) + " " + Double.isNaN(nan) + " " + Double.compare(nan, 1.0) + " " + Math.max(nan, 1) + " " + Math.min(-0.0, 0.0));
        Object o = null;
        System.out.println("null concat " + o + " " + null);
        int[][] tri = new int[4][];
        for (int i = 0; i < 4; i++) { tri[i] = new int[i + 1]; tri[i][i] = i; }
        System.out.println("jagged " + tri[3].length + " " + tri[3][3] + " " + tri[0].length);
        Integer i1 = 127, i2 = 127, i3 = 128, i4 = 128;
        System.out.println("integer cache " + (i1 == i2) + " " + (i3 == i4) + " " + i3.equals(i4));
        String t = "a,b,,c";
        System.out.println("strings " + t.replace(',', ';') + " " + t.lastIndexOf(',') + " " + " pad ".trim() + "|" + "abc".compareTo("abd") + " " + "Hello".contains("ell") + " " + String.valueOf(3.25));
    }
}
