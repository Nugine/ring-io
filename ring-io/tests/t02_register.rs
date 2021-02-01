use std::fs::File;
use std::os::unix::io::AsRawFd;

use ring_io::Ring;

#[test]
fn t02_01_register_file() {
    let ring = Ring::with_entries(32).setup().unwrap();

    let file = File::open("Cargo.toml").unwrap();

    unsafe { ring.register_files(&[file.as_raw_fd()]).unwrap() };
    ring.unregister_files().unwrap();

    drop(file);

    drop(ring);
}

#[test]
fn t02_02_register_buffer() {
    let ring = Ring::with_entries(32).setup().unwrap();

    let mut buf: Vec<u8> = vec![0; 1024];

    {
        let iovecs = [libc::iovec {
            iov_base: buf.as_mut_ptr().cast(),
            iov_len: buf.len(),
        }];
        unsafe { ring.register_buffers(iovecs.as_ptr(), 1).unwrap() };
        ring.unregister_buffers().unwrap();
    }

    drop(buf);

    drop(ring);
}
