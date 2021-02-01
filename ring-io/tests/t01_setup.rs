use ring_io::Ring;

#[test]
fn t01_setup() {
    let ring = Ring::with_entries(32).setup().unwrap();
    dbg!(ring.ring_fd());
    dbg!(ring.cq_entries());
    dbg!(ring.sq_entries());
    drop(ring);
}
