use super::disk_vec::DiskVec;
use std::io;

/// A fixed-sized bit-vector that uses memory mapped as backstorage, so that the kernel
/// can swap pages in and out when memory is needed by other processes
pub struct DiskBitVec {
    pub bitmap: DiskVec<u8>,
}

impl DiskBitVec {
    /// Return a new bit vector with all zeros
    pub fn zeros(len: usize) -> io::Result<Self> {
        let bytes = if len % 8 == 0 { len / 8 } else { len / 8 + 1 };
        Ok(Self {
            bitmap: DiskVec::full(bytes, 0)?,
        })
    }

    /// Return the value at one offset
    pub fn get_bit(&self, offset: usize) -> bool {
        let byte = self.bitmap[offset >> 3];
        let bit = (byte >> (offset & 0b111)) & 0b1;
        bit != 0
    }

    /// Overwrite the value at one offset
    pub fn set_bit(&mut self, offset: usize, value: bool) {
        let byte = &mut self.bitmap[offset >> 3];
        if value {
            *byte = *byte | (1 << (offset & 0b111));
        } else {
            *byte = *byte & !(1 << (offset & 0b111));
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test() {
        let mut bitvec = DiskBitVec::zeros(80).unwrap();
        for offset in 0..80 {
            assert_eq!(bitvec.get_bit(offset), false);
        }

        bitvec.set_bit(17, true);
        assert_eq!(bitvec.get_bit(17), true);
        for offset in 0..80 {
            if offset != 17 {
                assert_eq!(bitvec.get_bit(offset), false);
            }
        }

        bitvec.set_bit(17, false);
        for offset in 0..80 {
            assert_eq!(bitvec.get_bit(offset), false);
        }
    }
}
