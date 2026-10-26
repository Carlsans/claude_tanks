// Differential test: prints results that the translated program must match.
interface Shape {
    double area();
    default String describe() { return getClass().getSimpleName() + " of area " + area(); }
}

abstract class Base implements Shape {
    static int created;
    final String name;
    Base(String name) { this.name = name; created++; }
    public String toString() { return name + "[" + area() + "]"; }
}

class Circle extends Base {
    double r;
    Circle(double r) { super("circle"); this.r = r; }
    public double area() { return Math.PI * r * r; }
}

class Rect extends Base {
    double w, h;
    Rect(double w, double h) { super("rect"); this.w = w; this.h = h; }
    public double area() { return w * h; }
    public String describe() { return "a " + w + "x" + h + " rectangle"; }
}

class Counter {
    static final int START = 7;
    static int[] table;
    static { table = new int[4]; for (int i = 0; i < table.length; i++) table[i] = START * i; }
}

class MyException extends RuntimeException {
    final int code;
    MyException(String m, int code) { super(m); this.code = code; }
}

public class Main {
    static int fib(int n) { return n < 2 ? n : fib(n - 1) + fib(n - 2); }

    static long mix(long a, int b) { return (a << b) ^ (a >>> (64 - b)) ^ (a >> 3); }

    static String sw(int k) {
        switch (k) {
            case 1: return "one";
            case 2: return "two";
            case 10: return "ten";
            case 1000: return "thousand";
            default: return "other";
        }
    }

    static int dense(int k) {
        switch (k) { case 0: return 5; case 1: return 6; case 2: return 7; case 3: return 8; default: return -1; }
    }

    static int thrower(int x) {
        try {
            if (x == 0) throw new MyException("zero", 42);
            return 100 / (x - 1);
        } catch (MyException e) {
            return e.code;
        } catch (ArithmeticException e) {
            return -7;
        } finally {
            System.out.println("finally " + x);
        }
    }

    public static void main(String[] args) {
        System.out.println("fib " + fib(20));
        System.out.println("ints " + (Integer.MAX_VALUE + 1) + " " + (-7 / 2) + " " + (-7 % 2) + " " + (7 >>> 1) + " " + (-7 >> 1) + " " + (-7 >>> 28));
        System.out.println("longs " + mix(123456789L, 13) + " " + (Long.MIN_VALUE - 1) + " " + (5L * -3L));
        System.out.println("doubles " + (1.0 / 3) + " " + (2.0 / 0) + " " + (0.0 / 0) + " " + 1e10 + " " + 1e-5 + " " + 100.0 + " " + 0.001 + " " + (float) 0.1 + " " + 3.0f);
        System.out.println("casts " + (int) 3.99 + " " + (int) -3.99 + " " + (int) 1e20 + " " + (long) Double.NaN + " " + (byte) 200 + " " + (char) 65 + " " + (short) 70000);
        System.out.println("math " + Math.round(2.5) + " " + Math.round(-2.5) + " " + Math.round(0.49999999999999994) + " " + Math.abs(-4) + " " + Math.max(3, 9) + " " + Math.floor(-1.5) + " " + Math.sqrt(2));
        int[] a = new int[5];
        for (int i = 0; i < a.length; i++) a[i] = i * i;
        int sum = 0;
        for (int v : a) sum += v;
        System.out.println("array sum " + sum);
        double[][] grid = new double[3][4];
        grid[2][3] = 1.5;
        System.out.println("grid " + grid.length + " " + grid[0].length + " " + grid[2][3]);
        Shape[] shapes = { new Circle(1), new Rect(2, 3), new Circle(0.5) };
        double total = 0;
        for (Shape s : shapes) { total += s.area(); System.out.println(s.describe()); }
        System.out.println("total " + total + " created " + Base.created);
        System.out.println("toString " + shapes[1]);
        System.out.println("counter " + Counter.table[3]);
        System.out.println("switch " + sw(1) + sw(2) + sw(10) + sw(1000) + sw(5) + " " + dense(2) + dense(9));
        System.out.println("throw " + thrower(0) + " " + thrower(1) + " " + thrower(3));
        try {
            int[] b = new int[2];
            b[5] = 1;
        } catch (ArrayIndexOutOfBoundsException e) {
            System.out.println("caught index");
        }
        try {
            Object o = "string";
            Integer i = (Integer) o;
            System.out.println("no " + i);
        } catch (ClassCastException e) {
            System.out.println("caught cast");
        }
        try {
            String s = null;
            s.length();
        } catch (NullPointerException e) {
            System.out.println("caught null");
        }
        StringBuilder sb = new StringBuilder();
        for (char c = 'a'; c <= 'e'; c++) sb.append(c).append(',');
        sb.setLength(sb.length() - 1);
        System.out.println("builder " + sb + " " + sb.reverse());
        String s = "Hello, World";
        System.out.println("string " + s.length() + " " + s.charAt(4) + " " + s.indexOf("World") + " " + s.substring(7) + " " + s.toUpperCase() + " " + s.hashCode() + " " + s.equals("Hello, World") + " " + (s == "Hello, World"));
        Object o = shapes[0];
        System.out.println("instanceof " + (o instanceof Circle) + " " + (o instanceof Rect) + " " + (o instanceof Shape));
        Integer boxed = 1000;
        int unboxed = boxed + 1;
        System.out.println("boxing " + unboxed + " " + boxed.equals(1000) + " " + Integer.valueOf(5).hashCode() + " " + Double.valueOf(2.5));
        long l = 1;
        for (int i = 0; i < 70; i++) l = l * 3 + i;
        System.out.println("overflow " + l);
        boolean flag = sum > 10 && total > 3;
        System.out.println("flag " + flag + " " + !flag);
        char[] cs = { 'x', 'y', 'z' };
        System.out.println(new String(cs) + " " + String.valueOf(cs, 1, 2).length());
    }
}
