use std::slice::SliceIndex;
use std::ops::{Deref, DerefMut};
use libc;

#[repr(transparent)]
pub struct LiquidVecRef<'alloc> { alloc: &'alloc mut BumpAlloc }

impl <'alloc> LiquidVecRef<'alloc> {
    /// Consume the vector and produce a slice that can still be used; it's length is now fixed
    #[inline(always)]
    pub fn freeze(self) -> &'alloc mut [u8] {
        unsafe {
            let ret = std::ptr::slice_from_raw_parts_mut(self.alloc.top_base, self.alloc.top_size);

            self.alloc.top_base = self.alloc.top_base.add(self.alloc.top_size);
            self.alloc.top_size = 0;

            &mut *ret
        }
    }

    #[inline(always)]
    fn extend_one(&mut self, item: u8) {
        unsafe {
            *self.alloc.top_base.add(self.alloc.top_size) = item;
            self.alloc.top_size += 1;
        }
    }

    #[inline(always)]
    fn extend_reserve(&mut self, additional: usize) {
        unsafe {
            libc::madvise(self.alloc.top_base.add(self.alloc.top_size) as _, additional, libc::MADV_WILLNEED);
        }
    }

    #[inline(always)]
    pub fn extend_from_slice(&mut self, items: &[u8]) {
        unsafe {
            std::ptr::copy(items.as_ptr(), self.alloc.top_base.add(self.alloc.top_size), items.len());
            self.alloc.top_size += items.len();
        }
    }

    #[inline(always)]
    pub fn extend_from_within<R>(&mut self, src: R) where R : std::slice::SliceIndex<[u8], Output = [u8]> {
        unsafe {
            self.extend_from_slice(&std::slice::from_raw_parts(self.alloc.top_base, self.alloc.top_size).as_ref()[src])
        }
    }

    #[inline(always)]
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

    #[inline(always)]
    pub fn truncate(&mut self, len: usize) {
        if len > self.alloc.top_size {
            return;
        }
        self.alloc.top_size = len
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.alloc.top_size
    }

    #[inline(always)]
    pub fn set_len(&mut self, new_len: usize) {
        self.alloc.top_size = new_len;
    }
}

impl <'alloc> std::borrow::Borrow<[u8]> for LiquidVecRef<'alloc> {
    #[inline(always)]
    fn borrow(&self) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(self.alloc.top_base, self.alloc.top_size)
        }
    }
}

impl <'alloc> std::borrow::BorrowMut<[u8]> for LiquidVecRef<'alloc> {
    #[inline(always)]
    fn borrow_mut(&mut self) -> &mut [u8] {
        unsafe {
            std::slice::from_raw_parts_mut(self.alloc.top_base, self.alloc.top_size)
        }
    }
}

impl <'alloc> Extend<u8> for LiquidVecRef<'alloc>  {
    #[inline(always)]
    fn extend<T: IntoIterator<Item=u8>>(&mut self, iter: T) {
        iter.into_iter().for_each(|b| self.extend_one(b))
    }
}

impl <'alloc, I: SliceIndex<[u8]>> std::ops::Index<I> for LiquidVecRef<'alloc>  {
    type Output = I::Output;
    #[inline(always)]
    fn index(&self, index: I) -> &Self::Output { std::ops::Index::index(self.deref(), index) }
}

impl <'alloc, I: SliceIndex<[u8]>> std::ops::IndexMut<I> for LiquidVecRef<'alloc>  {
    #[inline(always)]
    fn index_mut(&mut self, index: I) -> &mut Self::Output { std::ops::IndexMut::index_mut(self.deref_mut(), index) }
}

impl <'alloc> std::ops::Deref for LiquidVecRef<'alloc> {
    type Target = [u8];

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        unsafe {
            std::slice::from_raw_parts(self.alloc.top_base, self.alloc.top_size)
        }
    }
}

impl <'alloc> std::ops::DerefMut for LiquidVecRef<'alloc> {
    #[inline(always)]
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
    /// New Bump allocator with at most ~4GB of stuff in it
    pub fn new() -> Self {
        Self::new_with_address_space(32)
    }

    /// New Bump allocator with at most ~2^bits stuff in it
    pub fn new_with_address_space(bits: u8) -> Self {
        use libc::*;
        unsafe {
            //NOTE: Changed the flags because miri only supports MAP_PRIVATE | MAP_ANONYMOUS.
            // The original flags were MAP_SHARED | MAP_ANONYMOUS | MAP_NORESERVE, however, I think
            // the new configuration should be fine for the needs of this crate, and won't have a
            // performance impact based on what I've read.  This appears to be true on Mac, but
            // I haven't tested on Linux.
            let res = mmap(std::ptr::null_mut(), 1 << bits, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
            if res as i64 == -1 {
                let err = std::io::Error::last_os_error();
                panic!("freeze: mmap error occurred: {err}");
            }
            if res as i64 == 0 {
                panic!("freeze: mmap returned nullptr")
            }
            *(res as *mut BumpAlloc) = BumpAlloc {
                address_space: 1 << bits,
                top_base: (res as *mut u8).byte_add(size_of::<BumpAlloc>()),
                top_size: 0,
            };

            Self { ptr: res as *mut BumpAlloc }
        }
    }

    /// Gets the (custom) Vec ref that's currently able to be modified
    pub fn top(&self) -> LiquidVecRef {
        unsafe {
            LiquidVecRef {
                alloc: self.ptr.as_mut().unwrap_unchecked()
            }
        }
    }

    unsafe fn data_range(&self) -> &[u8] {
        let data_base = self.ptr.byte_add(size_of::<BumpAlloc>()) as *const u8;
        std::slice::from_raw_parts(data_base, self.data_size())
    }

    unsafe fn data_range_mut(&mut self) -> &mut [u8] {
        let data_base = self.ptr.byte_add(size_of::<BumpAlloc>()) as *mut u8;
        std::slice::from_raw_parts_mut(data_base, self.data_size())
    }

    /// The total number of data bytes allocated over the lifetime of the allocator
    pub fn data_size(&self) -> usize {
        unsafe {
            let data_base = self.ptr as usize + size_of::<BumpAlloc>();
            ((*self.ptr).top_base as usize - data_base) + (*self.ptr).top_size
        }
    }

    /// More than half of the address space is already used
    pub fn dangerous(&self) -> bool {
        unsafe {
            (self.data_size() + size_of::<BumpAlloc>()) > (*self.ptr).address_space/2
        }
    }

    pub fn shrink_to_allocated(&self) {
        unsafe {
            let last = (*self.ptr).top_base.add((*self.ptr).top_size);
            let page_size = libc::sysconf(libc::_SC_PAGESIZE) as usize;
            let fit = ((last as usize)/page_size + 1)*page_size;
            let clearing = (*self.ptr).address_space - (fit - self.ptr as usize);
            // println!("{:?} (data_size={}, address_space={}, page_size={}) and performing munmap({:?}, {})", self.ptr as usize, self.data_size(), (*self.ptr).address_space, page_size, fit, clearing);
            // Don't remove. Both approaches work (and should keep working)
            // assert_eq!(libc::mremap(self.ptr as _, (*self.ptr).address_space, (fit - self.ptr as usize), 0) as *mut u8, self.ptr as *mut u8);
            if libc::munmap(fit as _, clearing) == -1 {
                let err = std::io::Error::last_os_error();
                panic!("freeze: munmap error occurred: {err}");
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
    }

    //I was looking at the freeze crate, and it has some cool properties.  But it wasn't very
    // clear how it was supposed to be used, based on the interface.
    //
    // Consider this: (I know it's wrong, but I think it illustrates how people might think it works.)
    // ```
    // let alloc = BumpAllocRef::new();
    //
    // let mut vec1 = alloc.top();
    // let mut vec2 = alloc.top();
    //
    // vec1.extend_one(b'1');
    // vec2.extend_one(b'2');
    //
    // assert_eq!(vec1[0], b'1');
    // assert_eq!(vec2[0], b'2'); //WTF?!?
    // ```
    //
    // I.e. the fact that the API lets me make two LiquidVec objects makes it feel like they
    // should behave as independent objects. The fact that they're aliases to the same underlying
    // object is confusing.  Also, it can lead to UB; See the `try_aliasing_ub` test.
    //
    //It seems to me that there are two directions you could take this to make the API sound,
    // and also behave more like what people would expect (and maybe add some useful features along the way).
    //
    //The first option is to enforce at runtime that only one LiquidVec can exist at a time.  So
    // the `top` method would be more like `new_liquid_vec`, which would panic if there were an
    // unfrozen vec already out there.  And a `try_new` that would return an option.
    //
    //That has the added advantage that you could then allow each LiquidVec to take a generic `T`,
    // and the `T` types on different vecs could be different from each other within the same allocator.
    //
    //The second option is to make the LiquidVecs function as `Writers`. This option would allow
    // multiple Writer objects to exist, but enforce that all Writers must be dropped before a
    // `frozen` slice can be created.  So the allocator would keep a counter of outstanding writers.
    //
    //If you go with the second option, then I think you should choose a different name from `LiquidVec`
    // because `Vec` conveys the idea of some kind of ownership that doesn't exist in any conceptual
    // form with this option.  I think calling it a `BufferWriter` or something makes more sense,
    // especially because of the limit to `u8` data, which also cuts against the associations people
    // have with `Vec`.
    //
    #[test]
    fn try_aliasing_ub() {
        let alloc = BumpAllocRef::new();

        let mut liquid = alloc.top();
        liquid.extend(b"Good data".into_iter().cloned());
        let alias = &mut liquid[..];

        let frozen = alloc.top().freeze();
        assert_eq!(frozen, b"Good data");

        //Did I just modify const data with safe code?????
        alias.copy_from_slice(b"Bad data!");
        assert_eq!(frozen, b"Good data");
    }



}
