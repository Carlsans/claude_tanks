// Differential test: threads with wait/notify, join, sleep and interrupt.
// The output does not depend on scheduling, so the JVM and the translation
// (green threads) must agree.
import java.util.LinkedList;

public class Main {
    static final Object lock = new Object();
    static int produced, consumed;
    static final LinkedList<Integer> queue = new LinkedList<>();

    static class Counter extends Thread {
        int total;
        final int from;
        Counter(int from) { super("counter-" + from); this.from = from; }
        public void run() { for (int i = from; i < from + 100; i++) total += i; }
    }

    public static void main(String[] args) throws Exception {
        Thread producer = new Thread(() -> {
            for (int i = 0; i < 20; i++) {
                synchronized (lock) {
                    while (queue.size() >= 3) {
                        try { lock.wait(); } catch (InterruptedException e) { return; }
                    }
                    queue.add(i);
                    produced++;
                    lock.notifyAll();
                }
            }
        }, "producer");
        final int[] sum = { 0 };
        Thread consumer = new Thread() {
            public void run() {
                for (int i = 0; i < 20; i++) {
                    synchronized (lock) {
                        while (queue.isEmpty()) {
                            try { lock.wait(); } catch (InterruptedException e) { return; }
                        }
                        sum[0] += queue.removeFirst();
                        consumed++;
                        lock.notifyAll();
                    }
                }
            }
        };
        consumer.setName("consumer");
        producer.start();
        consumer.start();
        producer.join();
        consumer.join();
        System.out.println("queue " + sum[0] + " " + produced + " " + consumed + " " + producer.isAlive() + " " + consumer.getName());
        Counter a = new Counter(0), b = new Counter(1000);
        a.start();
        b.start();
        a.join();
        b.join();
        System.out.println("counters " + a.total + " " + b.total + " " + a.getName());
        Thread sleeper = new Thread(() -> {
            try {
                Thread.sleep(100000);
                System.out.println("woke normally");
            } catch (InterruptedException e) {
                System.out.println("interrupted " + Thread.currentThread().getName());
            }
        }, "sleeper");
        sleeper.start();
        Thread.sleep(20);
        sleeper.interrupt();
        sleeper.join();
        System.out.println("main " + Thread.currentThread().getName() + " " + Thread.interrupted() + " " + sleeper.isAlive());
        final Object gate = new Object();
        final boolean[] open = { false };
        Thread waiter = new Thread(() -> {
            synchronized (gate) {
                while (!open[0]) {
                    try { gate.wait(); } catch (InterruptedException e) { return; }
                }
            }
            System.out.println("gate passed");
        });
        waiter.start();
        Thread.yield();
        synchronized (gate) {
            open[0] = true;
            gate.notify();
        }
        waiter.join(5000);
        System.out.println("done " + waiter.isAlive());
    }
}
