// Differential test: lambdas, method references and Java 9+ string
// concatenation (both compile to invokedynamic). Compiled for Java 17.
import java.util.*;
import java.util.function.*;

public class Main {
    interface Scorer { double score(double distance, int hits); }

    static int twice(int x) { return 2 * x; }

    static class Shot {
        final String target;
        final double power;
        Shot(String target, double power) { this.target = target; this.power = power; }
        String target() { return target; }
        double power() { return power; }
        public String toString() { return "Shot[" + target + ", " + power + "]"; }
    }

    final int base = 10;

    int addBase(int x) { return x + base; }

    public static void main(String[] args) {
        int bonus = 3;
        Scorer s = (d, h) -> h * 10 - d / 100 + bonus;
        System.out.println("scorer " + s.score(250, 4));
        IntUnaryOperator f = Main::twice;
        IntBinaryOperator add = Integer::sum;
        System.out.println("ops " + f.applyAsInt(21) + " " + add.applyAsInt(2, 3) + " " + f.andThen(x -> x + 1).applyAsInt(5));
        Main m = new Main();
        Function<Integer, Integer> bound = m::addBase;
        Supplier<List<String>> make = ArrayList::new;
        List<String> names = make.get();
        names.add("Walls");
        names.add("SpinBot");
        names.add("Crazy");
        names.add("RamFire");
        BiFunction<String, Integer, Character> charAt = String::charAt;
        System.out.println("refs " + bound.apply(5) + " " + names.size() + " " + charAt.apply("robot", 2));
        names.sort((a, b) -> a.length() != b.length() ? a.length() - b.length() : a.compareTo(b));
        System.out.println("sorted " + names);
        names.sort(Comparator.comparing((String n) -> n.toLowerCase()).reversed());
        System.out.println("comparing " + names);
        names.removeIf(n -> n.startsWith("C"));
        StringBuilder sb = new StringBuilder();
        names.forEach(n -> sb.append(n.charAt(0)));
        System.out.println("forEach " + sb + " " + names);
        Map<String, Integer> counts = new HashMap<>();
        for (String w : "a b a c b a".split(" ")) counts.merge(w, 1, Integer::sum);
        System.out.println("merge " + counts);
        Runnable r = () -> System.out.println("run " + bonus);
        r.run();
        Predicate<String> longName = n -> n.length() > 5;
        System.out.println("predicate " + longName.test("SpinBot") + " " + longName.negate().test("SpinBot"));
        Shot shot = new Shot("Walls", 1.9);
        long big = 1L << 40;
        char c = 'q';
        float fl = 2.5f;
        boolean yes = true;
        Object nothing = null;
        System.out.println("concat " + shot.target() + ":" + shot.power() + " " + big + c + fl + yes + nothing + " " + 7 + 'x' + 1.5);
        System.out.println("object " + shot);
        Optional<String> first = Optional.ofNullable(names.isEmpty() ? null : names.get(0));
        System.out.println("optional " + first.orElse("none") + " " + first.map(String::length).orElse(-1));
    }
}
