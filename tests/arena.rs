use std::sync::Arc;
use std::thread;

use silva::Arena;

macro_rules! assert_ptr_eq {
    ($left:expr, $right:expr $(,)?) => {
        assert_eq!(
            $left.as_ptr(),
            $right.as_ptr()
        );
    };

    ($left:expr, $right:expr, $($arg:tt)+) => {
        assert_eq!(
            $left.as_ptr(),
            $right.as_ptr(),
            $($arg)+
        );
    };
}

trait AsPtr<T> {
    fn as_ptr(&self) -> *const T;
}

impl<T> AsPtr<T> for &T {
    fn as_ptr(&self) -> *const T {
        *self as *const _
    }
}

impl<T> AsPtr<T> for Option<&T> {
    fn as_ptr(&self) -> *const T {
        self.map_or(std::ptr::null(), std::ptr::from_ref)
    }
}

#[test]
fn simple() {
    let arena = Arc::new(Arena::new());
    let root = arena.push(None, "root");

    let a = arena.push(root, "one");
    let b = arena.handle(arena.push(root, "two"));
    let c = arena.push(root, "three").index();

    assert_eq!(a.value, "one");
    assert_eq!(b.value, "two");
    assert_eq!(arena[c].value, "three");

    assert_ptr_eq!(a.parent(), Some(root));
    assert_ptr_eq!(b.parent(), Some(root));
    assert_ptr_eq!(arena[c].parent(), Some(root));

    let mut children = root.children();
    assert_eq!(children.next().unwrap().index(), c);
    assert_eq!(children.next().unwrap().index(), b.index());
    assert_eq!(children.next().unwrap().index(), a.index());

    drop(arena);
    assert_eq!(b.value, "two");
    assert!(b.parent().is_some());
}

#[test]
fn parallel_write() {
    let arena = Arc::new(Arena::new());
    let root = arena.push(None, 0).index();

    let v1 = arena.clone();
    let v2 = arena.clone();

    let t1 = thread::spawn(move || v1.push(root, 1).index());
    let t2 = thread::spawn(move || v2.push(root, 2).index());

    let i1 = t1.join().unwrap();
    let i2 = t2.join().unwrap();

    assert_eq!(arena[i1].value, 1);
    assert_eq!(arena[i2].value, 2);
    assert_eq!(arena.count(), 3);
}

#[test]
fn stress() {
    let n = thread::available_parallelism().unwrap().get();
    let total = if cfg!(miri) { n * 2 } else { n.pow(2) };
    let step = total / n;

    let arena = Arena::new();
    let root = arena.push(None, 0).index();

    let barrier = std::sync::Barrier::new(n);
    thread::scope(|s| {
        let arena = &arena;
        let barrier = &barrier;
        for i in 0..n {
            s.spawn(move || {
                for i in i * step..(i + 1) * step {
                    barrier.wait();
                    arena.push(root, i);
                }
            });
        }
    });

    assert_eq!(arena.count(), total + 1);
    // panic!("{:#?}", root.children().collect::<Vec<_>>());
}

#[test]
fn mt_read_write() {
    use std::sync::mpsc;

    let n = thread::available_parallelism().unwrap().get();
    let arena = Arena::new();
    let root = arena.push(None, 0).index();
    let total = if cfg!(miri) { n * 2 } else { n.pow(2) };
    let step = total / n;

    let (tx, rx) = mpsc::channel();
    let tx = Arc::new(tx);

    thread::scope(|s| {
        let arena = &arena;
        for i in 0..n {
            let tx = tx.clone();
            s.spawn(move || {
                for i in i * step..(i + 1) * step {
                    tx.send((arena.push(root, i).index(), i)).unwrap();
                }
                drop(tx);
            });
        }
        drop(tx);

        s.spawn(|| {
            for (node, value) in rx {
                assert_eq!(arena[node].value, value);
            }
        });
    });

    assert_eq!(arena.count(), total + 1);
}

#[test]
fn tree_macro() {
    let root;
    let root2;
    let one;

    let arena = Arena::new();
    silva::tree![
        &arena,
        root = ("root") = [
            ("one") = [],
            ("two"),
            ("three") = [],
            ("four"),
            ("five") //
        ],
        root2 = ("root2") = [
            one = ("one") = [
                ("two") = [("two two")],
                ("three"),
                ("four"),
                ("five") //
            ] //
        ]
    ];
    assert_eq!(arena.count(), 13);
    assert_eq!(root.value, "root");

    let names = ["one", "two", "three", "four", "five"];
    for (child, name) in root.children().zip(names.into_iter().rev()) {
        assert_eq!(name, child.value);
    }

    assert_ptr_eq!(root2.child(), Some(one));
    assert_eq!(root2.value, "root2");
    assert_eq!(one.value, "one");

    for (child, name) in one.children().zip(names[1..].into_iter().rev()) {
        assert_eq!(*name, child.value);
    }
}

#[test]
fn iter() {
    let arena = Arena::new();
    let root = arena.push(None, 0);
    let c = arena.push(root, 2);
    let b = arena.push(root, 1);
    let a = arena.push(root, 0);

    arena.push(c, 3);
    let n = [a, b, c];

    for (i, child) in root.children().enumerate() {
        assert_eq!(child.value, i);
        assert_ptr_eq!(n[i], child);
    }
}

#[test]
fn capacity_reserve() {
    let arena = Arena::<()>::with_capacity(0);
    for i in 0..SLOTS {
        arena.reserve(i);
        assert_eq!(arena.capacity(), SLOTS);
    }
    for i in SLOTS..SLOTS * 3 {
        arena.reserve(i);
        assert_eq!(arena.capacity(), SLOTS * 3);
    }
    for i in SLOTS * 3..SLOTS * 7 {
        arena.reserve(i);
        assert_eq!(arena.capacity(), SLOTS * 7);
    }
}

#[test]
fn unused_cap() {
    let arena = Arena::with_capacity(10_000);
    let mut prev = None;
    (0..100)
        .map(|i| {
            let node = arena.push(prev, i.to_string());
            prev = Some(node.index());
            node
        })
        .enumerate()
        .for_each(|(i, node)| assert_eq!(i.to_string(), node.value));
}

#[test]
fn deeply_nested() {
    let mut nodes = Vec::new();
    let mut indices = Vec::new();

    let arena = Arena::new();
    let mut parent = None;

    for i in 0..1_000 {
        let node = arena.push(parent, i);
        nodes.push(node);
        indices.push(node.index());
        if i % 10 == 0 {
            parent = Some(node.index());
        }
    }

    for i in 0..indices.len() {
        assert_eq!(arena[indices[i]].value, i);
        assert_eq!(nodes[i].value, i);
        assert_ptr_eq!(&nodes[i], &arena[indices[i]]);
    }
}

// taken from arena::raw
pub const SLOTS: usize = usize::BITS as usize;
