// Differential test: the parts of the Java library robots use most,
// translated from the JDK's own class files.
import java.awt.geom.*;
import java.util.*;

public class Main {
    static class Wave implements Comparable<Wave> {
        final double x, y, speed;
        Wave(double x, double y, double speed) { this.x = x; this.y = y; this.speed = speed; }
        public int compareTo(Wave o) { return Double.compare(speed, o.speed); }
        public String toString() { return "Wave(" + x + "," + y + "," + speed + ")"; }
    }

    public static void main(String[] args) {
        ArrayList<Integer> list = new ArrayList<>();
        for (int i = 0; i < 20; i++) list.add(i * 7 % 11);
        list.remove(Integer.valueOf(3));
        list.remove(0);
        int sum = 0;
        for (int v : list) sum += v;
        System.out.println("list " + list.size() + " " + sum + " " + list.get(4) + " " + list.indexOf(9) + " " + list.contains(10) + " " + list);
        Collections.sort(list);
        System.out.println("sorted " + list);
        HashMap<String, Integer> map = new HashMap<>();
        String[] words = { "fire", "aim", "move", "fire", "scan", "aim", "fire" };
        for (String w : words) map.put(w, map.containsKey(w) ? map.get(w) + 1 : 1);
        System.out.println("map " + map.size() + " " + map.get("fire") + " " + map.get("none") + " " + map);
        for (Map.Entry<String, Integer> e : map.entrySet()) System.out.println("  " + e.getKey() + "=" + e.getValue());
        LinkedList<Wave> waves = new LinkedList<>();
        waves.add(new Wave(1, 2, 14));
        waves.addFirst(new Wave(3, 4, 11));
        waves.add(new Wave(5, 6, 19.7));
        Iterator<Wave> it = waves.iterator();
        while (it.hasNext()) { Wave w = it.next(); if (w.speed > 15) it.remove(); }
        System.out.println("waves " + waves.size() + " " + waves.getFirst() + " " + waves.peekLast());
        Wave[] arr = { new Wave(0, 0, 3), new Wave(0, 0, 1), new Wave(0, 0, 2) };
        Arrays.sort(arr);
        System.out.println("sorted waves " + Arrays.toString(arr));
        double[] ds = { 3.5, -1, 2.25, 0 };
        Arrays.sort(ds);
        System.out.println("doubles " + Arrays.toString(ds) + " " + Arrays.binarySearch(ds, 2.25));
        Vector<String> vec = new Vector<>();
        vec.addElement("a");
        vec.add("b");
        vec.insertElementAt("c", 0);
        System.out.println("vector " + vec + " " + vec.elementAt(1) + " " + vec.lastElement());
        Hashtable<String, Double> table = new Hashtable<>();
        table.put("x", 1.5);
        table.put("y", 2.5);
        System.out.println("hashtable " + table.get("y") + " " + table.size() + " " + table.containsKey("z"));
        Random r = new Random(42);
        System.out.println("random " + r.nextInt(100) + " " + r.nextDouble() + " " + r.nextBoolean() + " " + r.nextGaussian() + " " + r.nextLong());
        Point2D.Double p = new Point2D.Double(3, 4);
        Point2D q = new Point2D.Double(0, 0);
        System.out.println("point " + p.distance(q) + " " + p.distanceSq(1, 1) + " " + p.getX() + " " + p);
        Rectangle2D.Double field = new Rectangle2D.Double(18, 18, 764, 564);
        System.out.println("rect " + field.contains(p) + " " + field.contains(100, 100) + " " + field.getCenterX() + " " + field.getMaxY() + " " + field.intersects(0, 0, 20, 20));
        Line2D.Double line = new Line2D.Double(0, 0, 10, 10);
        System.out.println("line " + line.ptSegDist(5, 0) + " " + line.intersectsLine(0, 10, 10, 0) + " " + line.getP2());
        ArrayList<double[]> history = new ArrayList<>();
        for (int i = 0; i < 5; i++) history.add(new double[] { i, i * i });
        double best = 0;
        for (double[] h : history) best = Math.max(best, h[1]);
        System.out.println("history " + history.size() + " " + best);
        TreeMap<Double, String> tree = new TreeMap<>();
        tree.put(2.0, "two");
        tree.put(-1.0, "minus");
        tree.put(5.5, "five");
        System.out.println("tree " + tree + " " + tree.firstKey() + " " + tree.floorKey(3.0) + " " + tree.headMap(2.0));
        StringBuilder sb = new StringBuilder();
        for (String s : new TreeSet<>(Arrays.asList(words))) sb.append(s).append(' ');
        System.out.println("treeset " + sb.toString().trim());
    }
}
