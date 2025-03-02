# Freeze
Currently, provides a simple bump-allocator that allows the top vector to be temporarily mutable, before being frozen as the new vector is allocated.

```rs
let mut alloc = BumpAllocRef::new();

let s1: &[u8] = {
    let mut v1: LiquidVecRef = alloc.top();
    v1.extend_from_slice(&[1, 2, 3]);
    v1.extend_one(4);
    v1.extend_from_within(..3);
    v1.deref_mut().reverse();
    v1.pop();
    v1.freeze()
};

assert_eq!(s1, [3, 2, 1, 4, 3, 2]);

let s2: &[u8] = {
    let mut v1: LiquidVecRef = alloc.top();
    v1.extend_from_slice(&[10, 20, 30]);
    v1.extend_one(40);
    v1.extend_from_within(..3);
    v1.deref_mut().reverse();
    v1.pop();
    v1.freeze()
};

assert_eq!(s2, [30, 20, 10, 40, 30, 20]);
assert_eq!(alloc.data_size(), (s1.len() + s2.len()));
```
