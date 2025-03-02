use std::ffi::CString;
use std::slice::SliceIndex;
use std::ops::{Deref, DerefMut};
use libc;

pub struct LiquidVecRef<'alloc> { alloc: &'alloc mut BumpAlloc }

impl <'alloc> LiquidVecRef<'alloc> {
    pub fn finalize(self) -> &'alloc mut [u8] {
        unsafe {
            let ret = std::ptr::slice_from_raw_parts_mut(self.alloc.top_base, self.alloc.top_size);

            self.alloc.top_base = self.alloc.top_base.add(self.alloc.top_size);
            self.alloc.top_size = 0;

            &mut *ret
        }
    }

    fn extend_one(&mut self, item: u8) {
        unsafe {
            *self.alloc.top_base.add(self.alloc.top_size) = item;
            self.alloc.top_size += 1;
        }
    }

    fn extend_reserve(&mut self, additional: usize) {
        unsafe {
            libc::madvise(self.alloc.top_base.add(self.alloc.top_size) as _, additional, libc::MADV_WILLNEED);
        }
    }

    pub fn extend_from_slice(&mut self, items: &[u8]) {
        unsafe {
            std::ptr::copy(items.as_ptr(), self.alloc.top_base.add(self.alloc.top_size), items.len());
            self.alloc.top_size += items.len();
        }
    }

    pub fn extend_from_within<R>(&mut self, src: R) where R : std::slice::SliceIndex<[u8], Output = [u8]> {
        unsafe {
            self.extend_from_slice(&std::slice::from_raw_parts(self.alloc.top_base, self.alloc.top_size).as_ref()[src])
        }
    }

    #[inline]
    pub fn pop(&mut self) -> Option<u8> {
        if self.alloc.top_size == 0 {
            None
        } else {
            unsafe {
                self.alloc.top_size -= 1;
                Some(std::ptr::read(self.alloc.top_base.add(self.alloc.top_size)))
            }
        }
    }

    pub fn truncate(&mut self, len: usize) {
        if len > self.alloc.top_size {
            return;
        }
        self.alloc.top_size = len
    }
}

impl <'alloc> std::borrow::Borrow<[u8]> for LiquidVecRef<'alloc> {
    fn borrow(&self) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(self.alloc.top_base, self.alloc.top_size)
        }
    }
}

impl <'alloc> std::borrow::BorrowMut<[u8]> for LiquidVecRef<'alloc> {
    fn borrow_mut(&mut self) -> &mut [u8] {
        unsafe {
            std::slice::from_raw_parts_mut(self.alloc.top_base, self.alloc.top_size)
        }
    }
}

impl <'alloc> Extend<u8> for LiquidVecRef<'alloc>  {
    fn extend<T: IntoIterator<Item=u8>>(&mut self, iter: T) {
        iter.into_iter().for_each(|b| self.extend_one(b))
    }
}

impl <'alloc, I: SliceIndex<[u8]>> std::ops::Index<I> for LiquidVecRef<'alloc>  {
    type Output = I::Output;
    fn index(&self, index: I) -> &Self::Output { std::ops::Index::index(self.deref(), index) }
}

impl <'alloc, I: SliceIndex<[u8]>> std::ops::IndexMut<I> for LiquidVecRef<'alloc>  {
    fn index_mut(&mut self, index: I) -> &mut Self::Output { std::ops::IndexMut::index_mut(self.deref_mut(), index) }
}

impl <'alloc> std::ops::Deref for LiquidVecRef<'alloc> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        unsafe {
            std::slice::from_raw_parts(self.alloc.top_base, self.alloc.top_size)
        }
    }
}

impl <'alloc> std::ops::DerefMut for LiquidVecRef<'alloc> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe {
            std::slice::from_raw_parts_mut(self.alloc.top_base, self.alloc.top_size)
        }
    }
}


struct BumpAlloc {
    address_space: usize,
    top_base: *mut u8,
    top_size: usize
}

#[repr(transparent)]
pub struct BumpAllocRef {
    ptr: *mut BumpAlloc
}

impl BumpAllocRef {
    fn new() -> Self {
        Self::new_with_address_space(32)
    }
    fn new_with_address_space(bits: u8) -> Self {
        use libc::*;
        unsafe {
            let res = mmap(std::ptr::null_mut(), 1 << bits, PROT_READ | PROT_WRITE, MAP_SHARED | MAP_ANONYMOUS | MAP_NORESERVE, -1, 0);
            if res as i64 == -1 {
                let cstring = strerror(*__errno_location());
                panic!("{:?}", CString::from_raw(cstring));
                // Err(std::str::from_utf8_unchecked(std::slice::from_raw_parts_mut(cstring, strlen(cstring)));
            }
            if res as i64 == 0 {
                panic!("mmap returned nullptr")
            }
            *(res as *mut BumpAlloc) = BumpAlloc {
                address_space: 1 << bits,
                top_base: (res as *mut u8).add(size_of::<BumpAlloc>()),
                top_size: 0,
            };

            Self { ptr: res as *mut BumpAlloc }
        }
    }

    fn top(&self) -> LiquidVecRef {
        unsafe {
            LiquidVecRef {
                alloc: self.ptr.as_mut().unwrap_unchecked()
            }
        }
    }
}

impl Drop for BumpAllocRef {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(self.ptr as _, (*self.ptr).address_space);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basis() {
        let alloc = BumpAllocRef::new();

        let s1 = {
            let mut v1 = alloc.top();
            v1.extend_from_slice(&[1, 2, 3]);
            v1.extend_one(4);
            v1.extend_from_within(..3);
            v1.deref_mut().reverse();
            v1.pop();
            v1.finalize()
        };

        assert_eq!(s1, [3, 2, 1, 4, 3, 2]);

        let s2 = {
            let mut v1 = alloc.top();
            v1.extend_from_slice(&[10, 20, 30]);
            v1.extend_one(40);
            v1.extend_from_within(..3);
            v1.deref_mut().reverse();
            v1.pop();
            v1.finalize()
        };

        assert_eq!(s2, [30, 20, 10, 40, 30, 20]);
    }
}
