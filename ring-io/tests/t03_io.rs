use ring_io::{PrepareSQE, Ring};

use std::fs::{self, File};
use std::os::unix::io::AsRawFd;
use std::ptr;

#[test]
fn t03_01_readv() {
    let ring = Ring::with_entries(32).setup().unwrap();
    let (mut sq, mut cq, _) = ring.split();

    assert_eq!(sq.available(), 32);
    assert_eq!(sq.prepared(), 0);
    assert_eq!(cq.ready(), 0);

    let file = File::open("Cargo.toml").unwrap();
    let mut buf = vec![0; 4096];
    let mut iovecs = [libc::iovec {
        iov_base: ptr::null_mut(),
        iov_len: 0,
    }];

    unsafe {
        let fd = file.as_raw_fd();
        let index = sq.pop_sqe().unwrap();

        assert_eq!(sq.available(), 31);
        assert_eq!(sq.prepared(), 0);

        iovecs[0] = libc::iovec {
            iov_base: buf.as_mut_ptr().cast(),
            iov_len: buf.len(),
        };
        sq.modify_sqe(index, |sqe| sqe.prep_readv(fd, iovecs.as_ptr(), 1, 0));
        sq.push_sqe(index);

        assert_eq!(sq.available(), 31);
        assert_eq!(sq.prepared(), 1);
    }

    let n_submitted = sq.submit_and_wait(1).unwrap();
    assert_eq!(n_submitted, 1);
    assert_eq!(sq.available(), 32);
    assert_eq!(sq.prepared(), 0);
    assert_eq!(cq.ready(), 1);

    let cqe = cq.pop_cqe().unwrap();
    dbg!(&cqe);

    assert_eq!(cq.ready(), 0);

    let result = cqe.io_result();
    dbg!(&result);

    let nread = result.unwrap() as usize;
    let data = &buf[..nread];

    let correct_data = fs::read("Cargo.toml").unwrap();

    assert_eq!(data, correct_data);

    drop((sq, cq));
    drop(file);
    drop(buf);
}
