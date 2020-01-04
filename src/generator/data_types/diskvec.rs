use memmap::MmapMut;
use std::io;
use std::marker::PhantomData;
use std::mem::size_of;
use std::ops::{Deref, DerefMut};
use tempfile::tempfile;

/// A fixed-sized vector that uses memory mapped as backstorage, so that the kernel
/// can swap pages in and out when memory is needed by other processes
pub struct DiskVec<T> {
    mem: MmapMut,
    len: usize,
    capacity: usize,
    phantom: PhantomData<T>,
}

impl<T> DiskVec<T> {
    pub fn new(capacity: usize) -> io::Result<Self> {
        let file = tempfile()?;
        file.set_len((capacity * size_of::<T>()) as u64)?;
        let mem = unsafe { MmapMut::map_mut(&file)? };
        Ok(Self {
            mem,
            len: 0,
            capacity,
            phantom: PhantomData,
        })
    }

    pub fn push(&mut self, value: T) {
        assert!(self.len < self.capacity);
        // Use write() because it was previously unitialized
        unsafe { self.as_mut_ptr().add(self.len).write(value) };
        self.len += 1;
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub unsafe fn as_ptr(&self) -> *const T {
        self.mem.as_ptr() as *const T
    }

    pub unsafe fn as_mut_ptr(&mut self) -> *mut T {
        self.mem.as_mut_ptr() as *mut T
    }
}

impl<T: Clone> DiskVec<T> {
    pub fn full(capacity: usize, value: T) -> io::Result<Self> {
        let mut vec = Self::new(capacity)?;
        unsafe {
            vec.len = capacity;
            for i in 0..capacity {
                vec.as_mut_ptr().add(i).write(value.clone());
            }
        }
        Ok(vec)
    }
}

impl<T> Deref for DiskVec<T> {
    type Target = [T];
    fn deref(&self) -> &Self::Target {
        // Safe because only the initialized values are exposed
        unsafe { std::slice::from_raw_parts(self.as_ptr(), self.len) }
    }
}

impl<T> DerefMut for DiskVec<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { std::slice::from_raw_parts_mut(self.as_mut_ptr(), self.len) }
    }
}

impl<T> Drop for DiskVec<T> {
    fn drop(&mut self) {
        unsafe {
            for i in 0..self.len {
                self.as_mut_ptr().add(i).drop_in_place()
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test() {
        let mut v = DiskVec::new(10).unwrap();
        for i in 0..10 {
            v.push(i);
        }

        assert_eq!(v[7], 7);
        assert_eq!(v.capacity(), 10);

        v[3] = 13;
        assert_eq!(
            v.iter().cloned().collect::<Vec<_>>(),
            vec![0, 1, 2, 13, 4, 5, 6, 7, 8, 9],
        );
    }

    #[test]
    #[should_panic]
    fn test_overflow() {
        let mut v = DiskVec::new(9).unwrap();
        for i in 0..10 {
            v.push(i);
        }
    }

    #[test]
    fn full() {
        let v = DiskVec::full(4, 3.14).unwrap();
        assert_eq!(
            v.iter().cloned().collect::<Vec<_>>(),
            vec![3.14, 3.14, 3.14, 3.14]
        );
    }
}
