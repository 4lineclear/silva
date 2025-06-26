use std::sync::Arc;
use std::thread;

use crate::Arena;

#[test]
fn simple() {
    let arena = Arc::new(Arena::new());
    let root = arena.push(None, "root").index();
    let a = arena.push(Some(&arena[root]), "one");
    let b = arena.handle(Some(&arena[root]), "two");
    let c = arena.push(Some(&arena[root]), "three").index();

    assert_eq!(a.value, "one");
    assert_eq!(b.value, "two");
    assert_eq!(arena[c].value, "three");

    assert_eq!(a.parent(), Some(root));
    assert_eq!(b.parent(), Some(root));
    assert_eq!(arena[c].parent(), Some(root));

    let mut children = arena[root].children(&arena);
    assert_eq!(children.next().unwrap().index(), c);
    assert_eq!(children.next().unwrap().index(), b.index());
    assert_eq!(children.next().unwrap().index(), a.index());

    drop(arena);
    assert_eq!(b.value, "two");
    assert!(b.arena().parent(&b).is_some());
}

#[test]
fn parallel_write() {
    let arena = Arc::new(Arena::new());
    let root = arena.push(None, 0).index();

    let v1 = arena.clone();
    let v2 = arena.clone();

    let t1 = thread::spawn(move || v1.push(Some(&v1[root]), 1).index());
    let t2 = thread::spawn(move || v2.push(Some(&v2[root]), 2).index());

    let i1 = t1.join().unwrap();
    let i2 = t2.join().unwrap();

    assert_eq!(arena[i1].value, 1);
    assert_eq!(arena[i2].value, 2);
    assert_eq!(arena.count(), 3);
}

#[test]
fn stress() {
    let arena = Arena::new();
    let barrier = std::sync::Barrier::new(4);
    let root = &arena.push(None, 0);
    let total = if cfg!(miri) { 100 } else { 10_000 };
    let step = total / 4;

    thread::scope(|s| {
        s.spawn(|| {
            for i in 0..step {
                barrier.wait();
                arena.push(Some(root), i);
            }
        });

        s.spawn(|| {
            for i in step..step * 2 {
                barrier.wait();
                arena.push(Some(root), i);
            }
        });

        s.spawn(|| {
            for i in step * 2..step * 3 {
                barrier.wait();
                arena.push(Some(root), i);
            }
        });

        s.spawn(|| {
            for i in step * 3..step * 4 {
                barrier.wait();
                arena.push(Some(root), i);
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
    crate::tree![
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
    for (child, name) in root.children(&arena).zip(names.into_iter().rev()) {
        assert_eq!(name, child.value);
    }

    assert_eq!(root2.child(), Some(one.index));
    assert_eq!(root2.value, "root2");
    assert_eq!(one.value, "one");

    for (child, name) in one.children(&arena).zip(names[1..].into_iter().rev()) {
        assert_eq!(*name, child.value);
    }
}

#[test]
fn capacity_reserve() {
    let arena = Arena::<()>::with_capacity(0);
    for i in 0..SLOTS {
        arena.reserve(i);
        assert_eq!(arena.capacity(), SLOTS);
        assert_eq!(Arena::<()>::with_capacity(i).capacity(), SLOTS);
    }
    for i in SLOTS..SLOTS * 3 {
        arena.reserve(i);
        assert_eq!(arena.capacity(), SLOTS * 3);
        assert_eq!(Arena::<()>::with_capacity(i).capacity(), SLOTS * 3);
    }
    for i in SLOTS * 3..SLOTS * 7 {
        arena.reserve(i);
        assert_eq!(arena.capacity(), SLOTS * 7);
        assert_eq!(Arena::<()>::with_capacity(i).capacity(), SLOTS * 7);
    }
}

#[test]
fn unused_cap() {
    let arena = Arena::with_capacity(100_000);
    let mut prev = None;
    (0..100)
        .map(|i| {
            let node = arena.push(prev, i.to_string());
            prev = Some(node);
            node
        })
        .enumerate()
        .for_each(|(i, node)| assert_eq!(i.to_string(), node.value));
}

// taken from arena::raw

/// The total number of slots
pub const SLOTS: usize = (usize::BITS / 2) as usize;
